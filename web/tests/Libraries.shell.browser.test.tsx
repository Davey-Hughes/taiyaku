import { act } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { page, userEvent } from 'vitest/browser'
// **Required**, the same reason `Config.browser.test.tsx` gives.
import '../src/index.css'
import { Config } from '../src/Config'
import * as config from '../src/config'
import * as curation from '../src/curation'
import { GateContext } from '../src/gate'
import * as librariesApi from '../src/libraries'
import * as library from '../src/library'
import { LibraryPage } from '../src/LibraryPage'
import { Shell } from '../src/Shell'
import { deferred, flush, settleUntil } from './support/async'
import {
  type Channels,
  channels,
  contrast,
  luminance,
  tokenColour,
} from './support/colour'
import { emptyDerive } from './support/derive'
import { holdOf } from './support/held'
import { press, submit, typeInto } from './support/input'
import { boxOf, expectKept } from './support/kept'
import { mount, rerender } from './support/mount'
import {
  holdTheClock,
  pass,
  WAIT_DRAWN_AFTER_MS,
  watchDrawn,
} from './support/wait'
import { row, section } from './support/settingsRows'
import * as usersApi from '../src/users'

/**
 * The Libraries screen inside the real `Shell`, and the Users screen's line
 * that focus lands on. Below 900px the document scrolls under a sticky phone
 * bar and above it `<main>` scrolls, and what a person can see of a report,
 * a refusal or a failure depends on both: a test shell without the bar
 * cannot tell where the bar covers. The last cases mount a library's page
 * alone, in a real browser for where it moves focus, which no bar changes.
 */

vi.mock('../src/library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/library')>()),
  status: vi.fn(),
}))
vi.mock('../src/curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/curation')>()),
  listProposals: vi.fn(),
}))
vi.mock('../src/config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/config')>()),
  patchRoot: vi.fn(),
  removalCost: vi.fn(),
  removeRoot: vi.fn(),
  scanStatus: vi.fn(),
  startScan: vi.fn(),
  listRoots: vi.fn(),
}))
vi.mock('../src/libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/libraries')>()),
  addDir: vi.fn(),
  createLibrary: vi.fn(),
  libraryRemovalCost: vi.fn(),
  libraryScanStatus: vi.fn(),
  listDirs: vi.fn(),
  listLibraries: vi.fn(),
  moveCost: vi.fn(),
  moveDir: vi.fn(),
  removeLibrary: vi.fn(),
  renameLibrary: vi.fn(),
  startLibraryScan: vi.fn(),
}))
vi.mock('../src/users', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/users')>()),
  listUsers: vi.fn(),
}))
// The build's label at the sidebar's foot asks for the build; answered here,
// so the request never leaves the test.
vi.mock('../src/about', async (importOriginal) => {
  const { ABOUT } = await import('./support/about')
  return {
    ...(await importOriginal<typeof import('../src/about')>()),
    about: vi.fn().mockResolvedValue(ABOUT),
  }
})

const TABLE = '/config/libraries'
/** The page that makes a library, outside the names' space. */
const NEW_LIBRARY = '/config/new-library'
/** Manga's page, where its own controls and its directories' table are. */
const MANGA_PAGE = `${TABLE}/Manga`
/** The page of Light novels' only directory, where its Move… is. */
const NOVELS_DIR_PAGE = `${TABLE}/Light%20novels/dirs/3`

const dir = (
  id: number,
  path: string,
  over: Partial<config.Root> = {},
): config.Root => ({
  id,
  path,
  edition_key: 'en',
  default_kind: 'manga',
  default_language: 'en',
  exclude_globs: null,
  scan_interval_secs: 86400,
  keeps_documents: false,
  last_scan_at: '2026-10-05T21:14:09Z',
  last_scan_status: 'ok',
  ...over,
})

const MANGA: librariesApi.Library = {
  id: 1,
  name: 'Manga',
  works: 1858,
  books: 0,
  dirs: [
    dir(1, '/mnt/user/media 1/Manga/Volumes'),
    dir(2, '/mnt/user/media 1/Manga/Chapters (weekly)', {
      edition_key: 'weekly',
      default_kind: 'chapter',
    }),
  ],
}
const NOVELS: librariesApi.Library = {
  id: 2,
  name: 'Light novels',
  works: 0,
  books: 14,
  dirs: [dir(3, '/mnt/user/media 1/Light Novels')],
}

/**
 * Libraries enough that the table is taller than the window at either size,
 * so that a page left from its foot is still scrolled there when the table
 * is drawn, and what lands at the table's head is out of view until
 * something brings it there.
 */
const SHELVES = Array.from(
  { length: 20 },
  (_, i): librariesApi.Library => ({
    id: 10 + i,
    name: `Shelf ${i + 1}`,
    works: 0,
    books: 0,
    dirs: [dir(10 + i, `/srv/shelf ${i + 1}`)],
  }),
)
/** Manga with directories enough that its page is taller than the window too. */
const LONG_MANGA: librariesApi.Library = {
  ...MANGA,
  dirs: [
    ...MANGA.dirs,
    ...Array.from({ length: 24 }, (_, i) =>
      dir(40 + i, `/mnt/user/media 1/Manga/Extras ${i + 1}`),
    ),
  ],
}
/** The list answers Light novels and the long Manga. */
const listLongManga = () =>
  vi.mocked(librariesApi.listLibraries).mockResolvedValue({
    libraries: [NOVELS, LONG_MANGA],
  })

/**
 * The list answers `libraries`, with Manga called `to` once a rename has
 * been sent, as the server's reads do: Manga's page is at its name, and a
 * read that still said the old one would leave it at an address that names
 * nothing.
 */
const listFollowingRename = (
  to: string,
  libraries: librariesApi.Library[] = [NOVELS, MANGA],
) =>
  vi.mocked(librariesApi.listLibraries).mockImplementation(async () => ({
    libraries: libraries.map((one) =>
      one.id === MANGA.id &&
      vi.mocked(librariesApi.renameLibrary).mock.calls.length > 0
        ? { ...one, name: to }
        : one,
    ),
  }))

const who = {
  user_id: 1,
  admin: true,
  can_download: true,
  can_stream: true,
  can_stamp: false,
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(library.status).mockResolvedValue({
    libraries: [{ id: 1, name: 'Volumes', scanned: true }],
    restricted: false,
  })
  vi.mocked(curation.listProposals).mockResolvedValue({
    proposals: [],
    counts: {},
  } as never)
  vi.mocked(config.scanStatus).mockImplementation((id) =>
    Promise.resolve({
      root_id: id,
      running: false,
      report: null,
      last_scan_at: null,
      last_scan_status: null,
    }),
  )
  vi.mocked(librariesApi.libraryScanStatus).mockImplementation((id) =>
    Promise.resolve({
      library_id: id,
      running: false,
      elapsed_secs: null,
      dirs: [],
    }),
  )
  vi.mocked(librariesApi.listLibraries).mockResolvedValue({
    libraries: [NOVELS, MANGA],
  })
  vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [] })
  vi.mocked(config.listRoots).mockResolvedValue({ roots: [] })
})

/**
 * A real mouse and real keys, inside `act` so that React sees what they start:
 * outside it every state update they cause is reported as not wrapped.
 */
const click = (el: Element) =>
  act(async () => {
    await userEvent.click(el)
  })
const fill = (el: Element, text: string) =>
  act(async () => {
    await userEvent.fill(el, text)
  })
const keys = (text: string) =>
  act(async () => {
    await userEvent.keyboard(text)
  })

async function settle(ms = 120) {
  await act(async () => {
    await new Promise((r) => setTimeout(r, ms))
  })
}

/** The screen at `entry`, inside the shell, at a window of the size given. */
async function shell(
  width: number,
  height: number,
  entry = TABLE,
): Promise<HTMLElement> {
  await page.viewport(width, height)
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <MemoryRouter initialEntries={[entry]}>
        <GateContext value={{ who, signOut: () => undefined }}>
          <Routes>
            <Route
              path="/config/*"
              element={
                <Shell>
                  <Config />
                </Shell>
              }
            />
          </Routes>
        </GateContext>
      </MemoryRouter>,
    )
    host.style.height = '100%'
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  await settle()
  await document.fonts.ready
  return host
}

/** To the foot of whatever scrolls: the document below 900px, `main` above. */
async function toTheFoot(host: HTMLElement) {
  const main = host.querySelector('main') as HTMLElement
  main.scrollTo(0, main.scrollHeight)
  window.scrollTo(0, document.documentElement.scrollHeight)
  await new Promise((r) => setTimeout(r, 60))
}

/** The bottom of the sticky phone bar; zero where there is none. */
const barBottom = () =>
  document.querySelector('.phone-bar')?.getBoundingClientRect().bottom ?? 0

/**
 * The list's failure, which every page draws at its own head: `on` is the
 * class of the page it is expected on.
 */
const listFailure = (host: HTMLElement, on: string) =>
  host.querySelector<HTMLElement>(`.${on} > [role="alert"]`)

/** The names in the table of libraries, in the order drawn. */
const rows = (host: HTMLElement) =>
  [...host.querySelectorAll('.library-table tbody .row-link')].map(
    (a) => a.textContent,
  )

/** The names of the libraries the sidebar lists. */
const sidebarNames = (host: HTMLElement) =>
  [...host.querySelectorAll('.sidebar-libraries a')].map((a) => a.textContent)

/** Removes the Manga library from the foot of its page. */
async function removeManga(host: HTMLElement) {
  vi.mocked(librariesApi.libraryRemovalCost).mockResolvedValue({
    removed: { works: 0, positions: 0, notes: 0, left_with_no_library: [] },
    derive: emptyDerive,
    dry_run: true,
  })
  vi.mocked(librariesApi.removeLibrary).mockResolvedValue({
    removed: { works: 0, positions: 0, notes: 0, left_with_no_library: [] },
    derive: { ...emptyDerive, lost_positions: 4 },
    dry_run: false,
  })
  await toTheFoot(host)
  await press(host.querySelector('button[name="remove-library"]') as Element)
  await press(host.querySelector('.remove-library .confirm') as Element)
  await settle()
}

const SIZES = [
  [390, 844],
  [1280, 800],
] as const

/** The top of the first line of `el`'s words. */
const wordsTop = (el: Element) => {
  const words = document.createRange()
  words.selectNodeContents(el)
  return words.getClientRects()[0].top
}

/** Whether `el` lies wholly in the window and clear of the phone bar. */
const expectInSight = (el: Element) => {
  const at = el.getBoundingClientRect()
  expect(at.top).toBeGreaterThanOrEqual(barBottom())
  expect(at.bottom).toBeLessThanOrEqual(window.innerHeight)
}

const STORE_LOCKED = new Error(
  '/api/library/libraries: 500 {"error":"the store is locked"}',
)

/**
 * The box that edits a library's name, in its page's title's place. Looked
 * up by its name alone, as are Save and the refusal below, so that what
 * measures the editor's place measures whatever form the rename takes.
 */
const nameBox = (host: HTMLElement) =>
  host.querySelector(
    '.library-page input[name="library-name"]',
  ) as HTMLInputElement | null

/** The rename's Save. */
const saveName = (host: HTMLElement) =>
  [...host.querySelectorAll<HTMLElement>('.library-page button')].find(
    (b) => b.textContent === 'Save',
  )

/** What the library page says went wrong, wherever it says it. */
const pageRefusal = (host: HTMLElement) =>
  host.querySelector<HTMLElement>('.library-page [role="alert"]')

/** A library's name long enough to wrap its title at every size. */
const WRAPPED_NAME =
  'Manga, every volume and every chapter of the weekly and the monthly runs'

/** Renames the Manga library with a real mouse and keys, and lets it settle. */
async function renameManga(host: HTMLElement, name = 'Manga 2') {
  await click(host.querySelector('button[name="rename-library"]') as Element)
  await fill(nameBox(host) as Element, name)
  await click(saveName(host) as Element)
  await settle(200)
}

/**
 * Renames Manga by script: no pointer or key moves the page, though the
 * editor's box and then the saved title are each brought into view as the
 * rename focuses them.
 */
async function renameMangaQuietly(host: HTMLElement) {
  await press(host.querySelector('button[name="rename-library"]') as Element)
  await typeInto(
    host.querySelector('form.rename-library input') as HTMLInputElement,
    'Manga 3',
  )
  await submit(host.querySelector('form.rename-library') as Element)
  await settle(200)
}

/**
 * Holds the list's next reads open, and hands back what fails them. A
 * rename brings its box and then the saved title into view, and with them
 * the place where the list's failure is drawn, directly beneath the page's
 * header; so a test of the failure's own reveal fails the read only once it
 * has put that place out of view.
 */
function holdReload() {
  const held =
    deferred<Awaited<ReturnType<typeof librariesApi.listLibraries>>>()
  vi.mocked(librariesApi.listLibraries).mockReturnValue(held.promise)
  return async (why: unknown) => {
    await flush(() => held.reject(why))
    await settle()
  }
}

/**
 * Scrolls whatever scrolls, the document below 900px and `main` above, until
 * the top of `el` is `y` from the top of the window.
 */
async function putAt(host: HTMLElement, el: Element, y: number) {
  const delta = el.getBoundingClientRect().top - y
  ;(host.querySelector('main') as HTMLElement).scrollBy(0, delta)
  window.scrollBy(0, delta)
  await new Promise((r) => setTimeout(r, 60))
  expect(Math.round(el.getBoundingClientRect().top)).toBe(y)
}

/** Opens the picker under Manga's directories with a real mouse. */
async function openAddDir(host: HTMLElement) {
  await click(host.querySelector('button[name="add-dir"]') as Element)
  return host.querySelector('.library-page .add-root') as HTMLElement
}

describe('the Configuration screens where focus only lands, in the real Shell', () => {
  it('draws no ring on the line an unknown account’s address lands on, and the ring on the back link the keyboard reaches from it', async () => {
    const host = await shell(390, 844, '/config/users/999')
    const line = host.querySelector('.missing') as HTMLElement
    expect(line.textContent).toBe(
      'There is no account at this address: it may have been removed.',
    )
    expect(document.activeElement).toBe(line)
    // Focus that came with no pointer is focus-visible, so this is the ring
    // the line would draw.
    expect(line.matches(':focus-visible')).toBe(true)
    expect(getComputedStyle(line).outlineStyle).toBe('none')

    await act(async () => {
      await userEvent.tab({ shift: true })
    })
    const back = host.querySelector('.page-header .back a') as HTMLElement
    expect(back.textContent).toBe('‹ Users')
    expect(document.activeElement).toBe(back)
    expect(back.matches(':focus-visible')).toBe(true)
    expect(getComputedStyle(back).outlineStyle).toBe('solid')

    // Handed focus by a script from a ringed control, the line is
    // focus-visible again, and still draws none.
    await act(async () => line.focus())
    expect(line.matches(':focus-visible')).toBe(true)
    expect(getComputedStyle(line).outlineStyle).toBe('none')
  })
})

describe('the Libraries screen in the real Shell', () => {
  it('draws a removal’s report clear of the phone bar, its opening sentence under nothing', async () => {
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [NOVELS, ...SHELVES, LONG_MANGA],
    })
    const host = await shell(390, 844, MANGA_PAGE)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [NOVELS, ...SHELVES],
    })
    await removeManga(host)
    const report = host.querySelector(
      '.library-table > .removal-report',
    ) as HTMLElement
    const sentence = report.querySelector('p') as HTMLElement
    expect(barBottom()).toBeGreaterThan(40)
    expect(report.getBoundingClientRect().top).toBeGreaterThanOrEqual(
      barBottom(),
    )
    const at = sentence.getBoundingClientRect()
    const hit = document.elementFromPoint(at.left + 20, at.top + 8)
    expect(hit === sentence || sentence.contains(hit)).toBe(true)
  })

  it('scrolls to a report only when its head is out of view', async () => {
    const reveal = vi.spyOn(Element.prototype, 'scrollIntoView')
    const host = await shell(1280, 800, MANGA_PAGE)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [NOVELS],
    })
    // At the top of a page that fits, the report is in view as it lands.
    vi.mocked(librariesApi.libraryRemovalCost).mockResolvedValue({
      removed: { works: 0, positions: 0, notes: 0, left_with_no_library: [] },
      derive: emptyDerive,
      dry_run: true,
    })
    vi.mocked(librariesApi.removeLibrary).mockResolvedValue({
      removed: { works: 0, positions: 0, notes: 0, left_with_no_library: [] },
      derive: emptyDerive,
      dry_run: false,
    })
    try {
      const main = host.querySelector('main') as HTMLElement
      main.scrollTo(0, 0)
      await press(
        host.querySelector('button[name="remove-library"]') as Element,
      )
      await press(host.querySelector('.remove-library .confirm') as Element)
      await settle()
      const report = host.querySelector(
        '.library-table > .removal-report',
      ) as HTMLElement
      expect(report.getBoundingClientRect().top).toBeGreaterThanOrEqual(0)
      expect(reveal).not.toHaveBeenCalled()
    } finally {
      reveal.mockRestore()
    }
  })

  for (const [width, height] of SIZES) {
    it(`draws the failure of the list that follows a removal in view with the report, at ${width}`, async () => {
      vi.mocked(librariesApi.listLibraries).mockResolvedValue({
        libraries: [NOVELS, ...SHELVES, LONG_MANGA],
      })
      const host = await shell(width, height, MANGA_PAGE)
      vi.mocked(librariesApi.listLibraries).mockRejectedValue(STORE_LOCKED)
      await removeManga(host)
      const alert = listFailure(host, 'library-table')
      expect(alert?.textContent).toContain('the store is locked')
      const at = (alert as Element).getBoundingClientRect()
      expect(at.top).toBeGreaterThanOrEqual(barBottom())
      expect(at.bottom).toBeLessThanOrEqual(window.innerHeight)
      const head = (
        host.querySelector('.library-table > .removal-report > p') as Element
      ).getBoundingClientRect()
      expect(head.top).toBeGreaterThanOrEqual(barBottom())
      expect(head.bottom).toBeLessThanOrEqual(window.innerHeight)
    })

    it(`draws a refused Add directory where the person is looking, at ${width}`, async () => {
      vi.mocked(librariesApi.addDir).mockRejectedValue(
        new Error(
          '/api/library/libraries/1/dirs: 400 {"error":"/mnt/user/media 1/Manga/Volumes/Extras is inside /mnt/user/media 1/Manga/Volumes, which is already a directory of the library `Manga`"}',
        ),
      )
      const host = await shell(width, height, MANGA_PAGE)
      const picker = await openAddDir(host)
      const box = picker.querySelector('input[name="path"]') as HTMLInputElement
      await click(box)
      await fill(box, '/mnt/user/media 1/Manga/Volumes/Extras')
      await click(picker.querySelector('button[name="pick"]') as Element)
      await settle()
      const alert = picker.querySelector('[role="alert"]') as HTMLElement
      expect(alert.textContent).toContain(
        ', which is already a directory of the library',
      )
      const at = alert.getBoundingClientRect()
      expect(at.top).toBeGreaterThanOrEqual(barBottom())
      expect(at.bottom).toBeLessThanOrEqual(window.innerHeight)
    })
  }

  it('gives the make form’s refusal the form’s own rhythm', async () => {
    vi.mocked(librariesApi.createLibrary).mockRejectedValue(
      new Error('/api/library/libraries: 400 {"error":"refused"}'),
    )
    const host = await shell(1280, 800, NEW_LIBRARY)
    const form = host.querySelector('form.add-library') as HTMLFormElement
    await fill(
      form.querySelector('input[name="library-name"]') as Element,
      'Comics',
    )
    await fill(
      form.querySelector('input[name="path"]') as Element,
      '/srv/comics',
    )
    await click(form.querySelector('button[type="submit"]') as Element)
    await settle()
    const alert = form.querySelector('[role="alert"]') as HTMLElement
    const submit = form.querySelector('button[type="submit"]') as HTMLElement
    const nameBox = form.querySelector(
      'input[name="library-name"]',
    ) as HTMLElement
    const pathLabel = row(form, 'Directory path').querySelector(
      '.settings-row-label',
    ) as HTMLElement
    // The refusal is a row of the form's box, beneath the buttons' row: its
    // words are as far below the button it answers as the Directory path
    // row's words are below the Name box, which fills its row. That is the
    // space a row keeps under what it holds, the line between two rows,
    // and the space a row keeps above what it holds.
    expect(wordsTop(alert) - submit.getBoundingClientRect().bottom).toBe(
      wordsTop(pathLabel) - nameBox.getBoundingClientRect().bottom,
    )
  })

  /**
   * O209: the pressed control at the very foot of the window, and its
   * refusal drawn beneath it, below the fold, is brought into view. Each
   * of these pages fits the window whole, and a control that is always in
   * view cannot be put at its foot, so the window is first made shorter
   * than the page down to the control, by `by`.
   */
  async function atTheFoot(width: number, el: HTMLElement, by = 100) {
    const main = document.querySelector('main') as HTMLElement
    const below =
      el.getBoundingClientRect().bottom + window.scrollY + main.scrollTop
    await page.viewport(width, Math.floor(below) - by)
    await settle()
    el.scrollIntoView({ block: 'end' })
    await new Promise((r) => setTimeout(r, 60))
    expect(
      Math.abs(el.getBoundingClientRect().bottom - window.innerHeight),
    ).toBeLessThan(2)
  }

  /** Opens Manga's name for editing, and types a name the server refuses. */
  async function refusedRename(width: number, height: number) {
    vi.mocked(librariesApi.renameLibrary).mockRejectedValue(
      new Error('/api/library/libraries/1: 400 {"error":"refused"}'),
    )
    listLongManga()
    const host = await shell(width, height, MANGA_PAGE)
    await click(host.querySelector('button[name="rename-library"]') as Element)
    await fill(nameBox(host) as Element, 'Manga 2')
    return { host, save: saveName(host) as HTMLElement }
  }

  // Save sits above the refusal's line at every width: beside the title at
  // a desk's, over the line at a phone's. Save is near the page's head, and
  // a window shorter than the page down to it would be shorter than the
  // refusal's own box at a desk's width: Save is put at the foot of a window
  // that ends with it.
  for (const [width, height] of SIZES) {
    it(`brings a refused rename into view from the foot, at ${width} — O209`, async () => {
      const { host, save } = await refusedRename(width, height)
      await atTheFoot(width, save, 0)
      await press(save)
      await settle()
      expectInSight(pageRefusal(host) as Element)
    })
  }

  for (const [width, height] of SIZES) {
    it(`brings the make form’s refusal into view from the foot, at ${width}`, async () => {
      vi.mocked(librariesApi.createLibrary).mockRejectedValue(
        new Error('/api/library/libraries: 400 {"error":"refused"}'),
      )
      const host = await shell(width, height, NEW_LIBRARY)
      const form = host.querySelector('form.add-library') as HTMLFormElement
      await fill(
        form.querySelector('input[name="library-name"]') as Element,
        'Comics',
      )
      await fill(
        form.querySelector('input[name="path"]') as Element,
        '/srv/comics',
      )
      const make = form.querySelector('button[type="submit"]') as HTMLElement
      await atTheFoot(width, make)
      await press(make)
      await settle()
      expectInSight(form.querySelector('[role="alert"]') as Element)
      expectInSight(make)
    })

    it(`brings a refused Add directory into view from the foot, at ${width}`, async () => {
      vi.mocked(librariesApi.addDir).mockRejectedValue(
        new Error('/api/library/libraries/1/dirs: 400 {"error":"refused"}'),
      )
      const host = await shell(width, height, MANGA_PAGE)
      const picker = await openAddDir(host)
      await fill(
        picker.querySelector('input[name="path"]') as Element,
        '/srv/extra',
      )
      const pick = picker.querySelector('button[name="pick"]') as HTMLElement
      await atTheFoot(width, pick)
      await press(pick)
      await settle()
      expectInSight(picker.querySelector('[role="alert"]') as Element)
    })

    it(`brings the list’s failure into view after a rename whose reload fails, at ${width}`, async () => {
      vi.mocked(librariesApi.renameLibrary).mockResolvedValue({
        id: 1,
        name: 'Manga 2',
      } as never)
      listLongManga()
      const host = await shell(width, height, MANGA_PAGE)
      // The read that follows the save is held while the page is taken to
      // its foot, where the failure's place is out of view, and fails only
      // then: nothing but the failure itself can bring it into view.
      const failReload = holdReload()
      await renameMangaQuietly(host)
      await toTheFoot(host)
      expect(
        (
          host.querySelector('.library-page > .page-header') as Element
        ).getBoundingClientRect().bottom,
      ).toBeLessThan(barBottom())
      await failReload(STORE_LOCKED)
      const alert = listFailure(host, 'library-page')
      expect(alert?.textContent).toContain('the store is locked')
      expectInSight(alert as Element)
    })

    /**
     * The make page fits the window, so the new library's page opens at its
     * head, with the failure's place in view: this holds where the failure
     * is drawn, and that it is seen. That a failure out of view is brought
     * into it is held by the rename's tests and by the removal's at 1280,
     * whose pages can be left scrolled away from that place.
     */
    it(`draws the list’s failure in sight on the new library’s page after a library is made and the reload fails, at ${width}`, async () => {
      vi.mocked(librariesApi.createLibrary).mockResolvedValue({
        library: {
          id: 3,
          name: 'Comics',
          works: 0,
          books: 0,
          dirs: [dir(9, '/srv/comics')],
        },
        added: [],
      })
      const host = await shell(width, height, NEW_LIBRARY)
      vi.mocked(librariesApi.listLibraries).mockRejectedValue(STORE_LOCKED)
      const form = host.querySelector('form.add-library') as HTMLFormElement
      await fill(
        form.querySelector('input[name="library-name"]') as Element,
        'Comics',
      )
      await fill(
        form.querySelector('input[name="path"]') as Element,
        '/srv/comics',
      )
      await click(form.querySelector('button[type="submit"]') as Element)
      await settle(200)
      // The library's own page, not the line saying there is none, which
      // draws the failure in the same place.
      expect(
        host.querySelector('.library-page > .page-header .page-title')
          ?.textContent,
      ).toBe('Comics')
      const alert = listFailure(host, 'library-page')
      expect(alert?.textContent).toContain('the store is locked')
      expectInSight(alert as Element)
    })
  }

  /**
   * O200: the sidebar lists the libraries `/api/library/status` answers,
   * which the shell reads when it mounts. Both reads here answer what the
   * store holds, and the store has the library only once the form has made
   * it, so the sidebar can learn of it only by asking again. The move to
   * the new library's page invalidates both addresses together, so each is
   * read once.
   */
  it('lists a library the form makes in the sidebar and on its page, with no reload, from one read of each', async () => {
    const MADE: librariesApi.Library = {
      id: 3,
      name: 'Manga',
      works: 0,
      books: 0,
      dirs: [dir(9, '/srv/manga')],
    }
    let made = false
    vi.mocked(librariesApi.createLibrary).mockImplementation(async () => {
      made = true
      return { library: MADE, added: [] }
    })
    vi.mocked(librariesApi.listLibraries).mockImplementation(async () => ({
      libraries: made ? [MADE] : [],
    }))
    vi.mocked(library.status).mockImplementation(async () => ({
      libraries: made ? [{ id: 3, name: 'Manga', scanned: false }] : [],
      restricted: false,
    }))
    const host = await shell(1280, 800, NEW_LIBRARY)
    expect(vi.mocked(library.status)).toHaveBeenCalled()
    expect(host.querySelector('.sidebar-libraries')).toBeNull()

    const form = host.querySelector('form.add-library') as HTMLFormElement
    await fill(
      form.querySelector('input[name="library-name"]') as Element,
      'Manga',
    )
    await fill(
      form.querySelector('input[name="path"]') as Element,
      '/srv/manga',
    )
    vi.mocked(librariesApi.listLibraries).mockClear()
    vi.mocked(library.status).mockClear()
    await click(form.querySelector('button[type="submit"]') as Element)
    await settle(200)

    const links = [
      ...host.querySelectorAll<HTMLAnchorElement>('.sidebar-libraries a'),
    ]
    expect(links.map((a) => a.textContent)).toEqual(['Manga'])
    expect(links[0]?.getAttribute('href')).toBe('/?library=3')
    expect(
      host.querySelector('.library-page > .page-header .page-title')
        ?.textContent,
    ).toBe('Manga')
    expect(librariesApi.listLibraries).toHaveBeenCalledTimes(1)
    expect(library.status).toHaveBeenCalledTimes(1)
  })

  /** The same, for a rename: the library is listed by its new name. */
  it('lists a renamed library by its new name in the sidebar and on its page, with no reload, from one read of each', async () => {
    const renamed = () =>
      vi.mocked(librariesApi.renameLibrary).mock.calls.length > 0
    vi.mocked(librariesApi.renameLibrary).mockResolvedValue({
      id: 1,
      name: 'Manga 2',
    } as never)
    vi.mocked(librariesApi.listLibraries).mockImplementation(async () => ({
      libraries: [NOVELS, renamed() ? { ...MANGA, name: 'Manga 2' } : MANGA],
    }))
    vi.mocked(library.status).mockImplementation(async () => ({
      libraries: [
        { id: 2, name: 'Light novels', scanned: true },
        { id: 1, name: renamed() ? 'Manga 2' : 'Manga', scanned: true },
      ],
      restricted: false,
    }))
    const host = await shell(1280, 800, MANGA_PAGE)
    expect(sidebarNames(host)).toEqual(['Light novels', 'Manga'])
    vi.mocked(librariesApi.listLibraries).mockClear()
    vi.mocked(library.status).mockClear()

    await renameManga(host)

    expect(
      host.querySelector('.library-page > .page-header .page-title')
        ?.textContent,
    ).toBe('Manga 2')
    expect(sidebarNames(host)).toEqual(['Light novels', 'Manga 2'])
    expect(librariesApi.listLibraries).toHaveBeenCalledTimes(1)
    expect(library.status).toHaveBeenCalledTimes(1)
  })

  /**
   * A rename reads the list at once, and here the read answers before the
   * move to the new address is drawn. Drawn at the old address, the new
   * name would name no library there.
   */
  it('draws no “No such library” between a rename’s answer and the move to its new address', async () => {
    vi.mocked(librariesApi.renameLibrary).mockResolvedValue({
      id: 1,
      name: 'Manga 2',
    } as never)
    listFollowingRename('Manga 2')
    const host = await shell(1280, 800, MANGA_PAGE)
    const missing = watchDrawn(() =>
      String(host.querySelector('.missing') !== null),
    )
    await renameManga(host)
    missing.stop()
    expect([...missing.seen]).toEqual(['false'])
    expect(
      host.querySelector('.library-page > .page-header .page-title')
        ?.textContent,
    ).toBe('Manga 2')
  })

  it('saves the name on Enter in the box, focusing the title with no ring, and closes the editor on Escape from the box or Save with focus back on Rename…', async () => {
    vi.mocked(librariesApi.renameLibrary).mockResolvedValue({
      id: 1,
      name: 'Manga 2',
    } as never)
    listFollowingRename('Manga 2')
    const host = await shell(1280, 800, MANGA_PAGE)
    const rename = () =>
      host.querySelector('button[name="rename-library"]') as HTMLElement
    await click(rename())
    expect(document.activeElement).toBe(nameBox(host))
    // The name is selected, so the keys replace it.
    await keys('Comics')
    expect(nameBox(host)?.value).toBe('Comics')
    await keys('{Escape}')
    expect(nameBox(host)).toBeNull()
    expect(document.activeElement).toBe(rename())

    await click(rename())
    await keys('{Tab}')
    expect(document.activeElement).toBe(saveName(host))
    await keys('{Escape}')
    expect(nameBox(host)).toBeNull()
    expect(document.activeElement).toBe(rename())

    await click(rename())
    await keys('Manga 2{Enter}')
    await settle(200)
    expect(librariesApi.renameLibrary).toHaveBeenCalledTimes(1)
    expect(librariesApi.renameLibrary).toHaveBeenCalledWith(1, 'Manga 2')
    expect(nameBox(host)).toBeNull()
    const title = host.querySelector('.library-page .page-title') as HTMLElement
    expect(document.activeElement).toBe(title)
    expect(title.textContent).toBe('Manga 2')
    expect(title.matches(':focus-visible')).toBe(true)
    expect(getComputedStyle(title).outlineStyle).toBe('none')
  })

  /** The same, for a removal: the removed library leaves the sidebar. */
  it('drops a removed library from the sidebar, with no reload', async () => {
    // The store holds Manga until the removal is confirmed.
    const removed = () =>
      vi.mocked(librariesApi.removeLibrary).mock.calls.length > 0
    vi.mocked(librariesApi.listLibraries).mockImplementation(async () => ({
      libraries: removed() ? [NOVELS] : [NOVELS, MANGA],
    }))
    vi.mocked(library.status).mockImplementation(async () => ({
      libraries: [
        { id: 2, name: 'Light novels', scanned: true },
        ...(removed() ? [] : [{ id: 1, name: 'Manga', scanned: true }]),
      ],
      restricted: false,
    }))
    const host = await shell(1280, 800, MANGA_PAGE)
    expect(sidebarNames(host)).toEqual(['Light novels', 'Manga'])

    await removeManga(host)

    expect(rows(host)).toEqual(['Light novels'])
    expect(sidebarNames(host)).toEqual(['Light novels'])
  })

  /**
   * The same, when the list cannot be read again after the removal: the
   * library is gone all the same, so the sidebar still asks.
   */
  it('drops a removed library from the sidebar when the list cannot be read again', async () => {
    const removed = () =>
      vi.mocked(librariesApi.removeLibrary).mock.calls.length > 0
    vi.mocked(librariesApi.listLibraries).mockImplementation(async () => {
      if (removed()) throw STORE_LOCKED
      return { libraries: [NOVELS, MANGA] }
    })
    vi.mocked(library.status).mockImplementation(async () => ({
      libraries: [
        { id: 2, name: 'Light novels', scanned: true },
        ...(removed() ? [] : [{ id: 1, name: 'Manga', scanned: true }]),
      ],
      restricted: false,
    }))
    const host = await shell(1280, 800, MANGA_PAGE)
    expect(sidebarNames(host)).toEqual(['Light novels', 'Manga'])

    await removeManga(host)

    expect(listFailure(host, 'library-table')?.textContent).toContain(
      'the store is locked',
    )
    expect(sidebarNames(host)).toEqual(['Light novels'])
  })

  /**
   * The same, for a move: moving a library's only directory to a new library
   * removes the one and makes the other, and the sidebar follows both.
   */
  it('follows a move that makes one library and removes another in the sidebar, with no reload', async () => {
    const moved = () => vi.mocked(librariesApi.moveDir).mock.calls.length > 0
    const EXTRAS: librariesApi.Library = {
      ...NOVELS,
      id: 3,
      name: 'Extras',
    }
    const answer = (dry_run: boolean): librariesApi.MoveAnswer => ({
      moved: {
        library_id: 3,
        library: 'Extras',
        works_kept: 0,
        works_joined: 0,
        works_split: 0,
        series_joined: 0,
        positions: 0,
        notes: 0,
        alignments_dropped: 0,
        library_removed: 'Light novels',
        grants_removed: 0,
        left_with_no_library: [],
      },
      derive: emptyDerive,
      dry_run,
    })
    vi.mocked(librariesApi.moveCost).mockResolvedValue(answer(true))
    vi.mocked(librariesApi.moveDir).mockResolvedValue(answer(false))
    vi.mocked(librariesApi.listLibraries).mockImplementation(async () => ({
      libraries: moved() ? [EXTRAS, MANGA] : [NOVELS, MANGA],
    }))
    vi.mocked(library.status).mockImplementation(async () => ({
      libraries: [
        moved()
          ? { id: 3, name: 'Extras', scanned: true }
          : { id: 2, name: 'Light novels', scanned: true },
        { id: 1, name: 'Manga', scanned: true },
      ],
      restricted: false,
    }))
    const host = await shell(1280, 800, NOVELS_DIR_PAGE)
    expect(sidebarNames(host)).toEqual(['Light novels', 'Manga'])

    const move = [...host.querySelectorAll('.dir-page button')].find(
      (b) => b.textContent === 'Move…',
    ) as Element
    await click(move)
    await act(async () => {
      await userEvent.selectOptions(
        host.querySelector('select[name="move-target"]') as HTMLSelectElement,
        'new',
      )
    })
    await fill(
      host.querySelector('input[name="new-library"]') as Element,
      'Extras',
    )
    await click(host.querySelector('button[name="rehearse"]') as Element)
    await settle()
    await click(host.querySelector('.move-root .confirm') as Element)
    await settle(200)

    expect(rows(host)).toEqual(['Extras', 'Manga'])
    expect(sidebarNames(host)).toEqual(['Extras', 'Manga'])
  })

  it('brings a failure into view again when the same sentence answers a second reload', async () => {
    vi.mocked(librariesApi.renameLibrary).mockResolvedValue({
      id: 1,
      name: 'Manga 2',
    } as never)
    listLongManga()
    const host = await shell(390, 844, MANGA_PAGE)
    vi.mocked(librariesApi.listLibraries).mockRejectedValue(STORE_LOCKED)
    await renameManga(host)
    const alert = () => listFailure(host, 'library-page') as HTMLElement
    expectInSight(alert())

    // Another rename, whose read is held until the page is taken to its
    // foot with the sentence still standing and out of view; the same
    // refusal then answers it.
    const failReload = holdReload()
    await renameMangaQuietly(host)
    await toTheFoot(host)
    expect(alert().getBoundingClientRect().bottom).toBeLessThan(barBottom())
    await failReload(STORE_LOCKED)
    expectInSight(alert())
  })

  it('brings a failure that sits beneath the phone bar out from under it', async () => {
    vi.mocked(librariesApi.renameLibrary).mockResolvedValue({
      id: 1,
      name: 'Manga 2',
    } as never)
    listLongManga()
    const host = await shell(390, 844, MANGA_PAGE)
    vi.mocked(librariesApi.listLibraries).mockRejectedValue(STORE_LOCKED)
    await renameManga(host)
    const alert = () => listFailure(host, 'library-page') as HTMLElement
    expectInSight(alert())

    // Another rename, whose read is held until the sentence is wholly in
    // the window and under the bar, not in view; the same refusal then
    // answers it.
    const failReload = holdReload()
    await renameMangaQuietly(host)
    await putAt(host, alert(), 20)
    expect(alert().getBoundingClientRect().top).toBeLessThan(barBottom())
    await failReload(STORE_LOCKED)
    expectInSight(alert())
  })

  for (const [width, height] of SIZES) {
    it(`brings a failure whose head is cut off by the foot of the window into view, at ${width}`, async () => {
      vi.mocked(librariesApi.renameLibrary).mockResolvedValue({
        id: 1,
        name: 'Manga 2',
      } as never)
      const host = await shell(width, height, MANGA_PAGE)
      vi.mocked(librariesApi.listLibraries).mockRejectedValue(STORE_LOCKED)
      await renameMangaQuietly(host)
      const alert = () => listFailure(host, 'library-page') as HTMLElement
      const top = Math.round(alert().getBoundingClientRect().top)
      expectInSight(alert())

      // A window just taller than the top of the sentence: the top is in it
      // and the head of the sentence is not.
      await page.viewport(width, top + 10)
      await settle()
      expect(alert().getBoundingClientRect().top).toBeLessThan(
        window.innerHeight,
      )
      expect(alert().getBoundingClientRect().top + 48).toBeGreaterThan(
        window.innerHeight,
      )
      await renameMangaQuietly(host)
      expectInSight(alert())
    })
  }

  it('does not scroll for the failure of the first read', async () => {
    const reveal = vi.spyOn(Element.prototype, 'scrollIntoView')
    try {
      vi.mocked(librariesApi.listLibraries).mockRejectedValue(STORE_LOCKED)
      const host = await shell(390, 844)
      expect(listFailure(host, 'library-table')?.textContent).toContain(
        'the store is locked',
      )
      expect(reveal).not.toHaveBeenCalled()
    } finally {
      reveal.mockRestore()
    }
  })

  for (const [width, height] of SIZES) {
    it(`does not scroll a person typing at the foot when a scan ends and the list cannot be read, at ${width}`, async () => {
      let scanning = true
      vi.mocked(librariesApi.startLibraryScan).mockResolvedValue({
        library_id: 1,
        status_url: '/x',
      })
      listLongManga()
      const host = await shell(width, height, MANGA_PAGE)
      vi.mocked(librariesApi.libraryScanStatus).mockImplementation((id) =>
        Promise.resolve({
          library_id: id,
          running: id === 1 && scanning,
          elapsed_secs: 1,
          dirs: [],
        }),
      )
      await click(host.querySelector('button[name="scan-library"]') as Element)
      await settle()
      const picker = await openAddDir(host)
      await toTheFoot(host)
      const box = picker.querySelector('input[name="path"]') as HTMLInputElement
      await click(box)
      await keys('/srv/com')
      const reveal = vi.spyOn(Element.prototype, 'scrollIntoView')
      try {
        vi.mocked(librariesApi.listLibraries).mockRejectedValue(STORE_LOCKED)
        scanning = false
        // The poll that finds the scan ended, and the list read after it.
        await settleUntil(
          'the list’s failure',
          () =>
            listFailure(host, 'library-page')?.textContent?.includes(
              'the store is locked',
            ) === true,
          3 * config.SCAN_POLL_MS,
        )
        // The wait above ends as soon as the failure is drawn, and a reveal
        // put off by a frame or a timer would come after that.
        await settle()
        // The page was not told to scroll to the sentence, and the box the
        // person is typing in is still where they can see it.
        expect(reveal).not.toHaveBeenCalled()
        expectInSight(box)
        expect(document.activeElement).toBe(box)
        await keys('ics')
        expect(box.value).toBe('/srv/comics')
      } finally {
        reveal.mockRestore()
      }
    })
  }

  it('leaves typing in the path box alone when a listing the person asked for arrives', async () => {
    const slow = deferred<librariesApi.DirListing>()
    vi.mocked(librariesApi.listDirs).mockImplementation((path) =>
      path === undefined
        ? Promise.resolve({
            path: null,
            parent: null,
            dirs: [{ name: '/mnt/user', path: '/mnt/user' }],
          })
        : slow.promise,
    )
    const host = await shell(1280, 800, MANGA_PAGE)
    const picker = await openAddDir(host)
    const box = picker.querySelector('input[name="path"]') as HTMLInputElement
    await click(picker.querySelector('button[name="browse"]') as Element)
    await settle()
    await click(picker.querySelector('.dir-list button') as Element)
    await click(box)
    await keys('/mnt/user/med')
    await act(async () => {
      slow.resolve({
        path: '/mnt/user',
        parent: null,
        dirs: [{ name: 'media 1', path: '/mnt/user/media 1' }],
      })
      await Promise.resolve()
      await Promise.resolve()
    })
    await settle()
    expect(document.activeElement).toBe(box)
    await keys('ia')
    expect(box.value).toBe('/mnt/user/media')
  })

  it('keeps a long path over a directory’s report inside its box, and the page from scrolling sideways, at 390', async () => {
    const long = `/mnt/user/media 1/Manga/${'Volumes'.repeat(12)}`
    let scanning = true
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [NOVELS, { ...MANGA, dirs: [dir(1, long), MANGA.dirs[1]] }],
    })
    // A scan of both of Manga's directories is running when the page is
    // drawn, so the page takes it up, and the poll after `scanning` goes
    // false ends it and reads each directory's report.
    vi.mocked(librariesApi.libraryScanStatus).mockImplementation((id) =>
      Promise.resolve({
        library_id: id,
        running: id === 1 && scanning,
        elapsed_secs: scanning ? 1 : null,
        dirs:
          id === 1
            ? [1, 2].map((root_id) => ({ root_id, running: scanning }))
            : [],
      }),
    )
    vi.mocked(config.scanStatus).mockImplementation((id) =>
      Promise.resolve({
        root_id: id,
        running: false,
        report: {
          scan: {
            root: id === 1 ? long : MANGA.dirs[1].path,
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
        last_scan_at: null,
        last_scan_status: null,
      }),
    )
    const host = await shell(390, 844, MANGA_PAGE)
    scanning = false
    const heading = () =>
      [...host.querySelectorAll<HTMLElement>('.dir-report > h4')].find(
        (h) => h.textContent === long,
      )
    await settleUntil(
      'the report under the long path',
      () => heading() !== undefined,
      3 * config.SCAN_POLL_MS,
    )
    const drawn = heading() as HTMLElement
    expect(drawn.scrollWidth).toBeLessThanOrEqual(drawn.clientWidth)
    const page = document.documentElement
    expect(page.scrollWidth).toBeLessThanOrEqual(page.clientWidth)
  })

  it('does not scroll sideways while the name is edited, and holds the box and Save · Cancel inside the header, at 320', async () => {
    const host = await shell(320, 700, MANGA_PAGE)
    await click(host.querySelector('button[name="rename-library"]') as Element)
    const box = nameBox(host) as HTMLInputElement
    expect(document.activeElement).toBe(box)
    const root = document.documentElement
    expect(root.scrollWidth).toBeLessThanOrEqual(root.clientWidth)
    const header = (
      host.querySelector('.library-page > .page-header') as HTMLElement
    ).getBoundingClientRect()
    const save = saveName(host) as HTMLElement
    const cancel = [...host.querySelectorAll<HTMLElement>('button')].find(
      (b) => b.textContent === 'Cancel',
    ) as HTMLElement
    // The whole box, its border and padding too, inside the title's column:
    // a box standing out of it is cut off where its column clips.
    const at = box.getBoundingClientRect()
    expect(at.left).toBeGreaterThanOrEqual(header.left - 0.5)
    expect(at.right).toBeLessThanOrEqual(header.right + 0.5)
    for (const el of [save, cancel]) {
      expect(el.getBoundingClientRect().right).toBeLessThanOrEqual(
        header.right + 0.5,
      )
    }
    // Save and Cancel on one line, as Rename… and Scan library are, not
    // Cancel wrapped beneath Save.
    expect(save.getBoundingClientRect().top).toBe(
      cancel.getBoundingClientRect().top,
    )
  })

  it('keeps the box, and the title a save hands focus to, clear of the phone bar when Rename… or Save is pressed just beneath it, at 390', async () => {
    vi.mocked(librariesApi.renameLibrary).mockResolvedValue({
      id: 1,
      name: 'Manga 2',
    } as never)
    listFollowingRename('Manga 2', [NOVELS, LONG_MANGA])
    const host = await shell(390, 844, MANGA_PAGE)
    const rename = host.querySelector(
      'button[name="rename-library"]',
    ) as HTMLElement
    await putAt(host, rename, Math.ceil(barBottom()) + 1)
    await click(rename)
    const box = nameBox(host) as HTMLInputElement
    expect(document.activeElement).toBe(box)
    expect(box.getBoundingClientRect().top).toBeGreaterThanOrEqual(barBottom())

    await fill(box, 'Manga 2')
    const save = saveName(host) as HTMLElement
    await putAt(host, save, Math.ceil(barBottom()) + 1)
    await press(save)
    await settle()
    const title = host.querySelector('.library-page .page-title') as HTMLElement
    expect(document.activeElement).toBe(title)
    expect(title.getBoundingClientRect().top).toBeGreaterThanOrEqual(
      barBottom(),
    )
  })
})

/**
 * Rename… lays its box over the title, which stays beneath it, hidden, and
 * swaps Rename… · Scan library for Save · Cancel in one slot, so that
 * nothing around the box moves: not the header's lines, not its buttons,
 * not the first box beneath it, and not the name's own words, which stand
 * where the title's did. A box is not as tall in every browser
 * (Firefox draws a text box no shorter than its face's own line, taller
 * than the title's), so each case is also measured with the box made
 * taller than the title.
 */
describe('Rename… over the title’s place, in the real Shell', () => {
  /**
   * Where Rename… is pressed: the library's page alone at a desk's width and
   * at a phone's, and in the list's column beside one of its directories,
   * narrower than the page's measure and scrolling by itself.
   */
  const PLACES = [
    { at: '1920', width: 1920, height: 900, beside: false },
    { at: '1366, beside a directory', width: 1366, height: 900, beside: true },
    { at: '390', width: 390, height: 844, beside: false },
  ] as const

  /** What is drawn around the box, each by a name a failure can say. */
  const around = (page: HTMLElement): Record<string, Element | null> => {
    const directories = section(page, 'Directories')
    return {
      'the back link': page.querySelector('.page-header .back a'),
      'the title': page.querySelector('.page-header .page-title'),
      'the description line': page.querySelector(
        '.page-header .page-description',
      ),
      'Scan library': page.querySelector('button[name="scan-library"]'),
      'Rename…': page.querySelector('button[name="rename-library"]'),
      'the Directories heading': directories.querySelector(
        '.settings-section-title',
      ),
      'the Directories box': directories.querySelector(
        ':scope > .settings-box',
      ),
    }
  }

  /**
   * `el`'s left, top, width and height from the page's own top left, so
   * that a scroll is not taken for a move.
   */
  const from = (page: HTMLElement, el: Element) => {
    const origin = page.getBoundingClientRect()
    const at = el.getBoundingClientRect()
    return [at.left - origin.left, at.top - origin.top, at.width, at.height]
  }

  const measure = (page: HTMLElement) =>
    Object.fromEntries(
      Object.entries(around(page)).map(([name, el]) => [
        name,
        el === null ? null : from(page, el),
      ]),
    )

  /**
   * Where `el`'s words start and the baseline they stand on, from the
   * page's own top left. A heading's are read from its text and a mark set
   * on its first line. A text box's text cannot be measured inside it, so
   * its start is where its content starts, less what it is scrolled by, and
   * its baseline that of a copy of it lined up on its baseline beside an
   * empty block, whose foot marks the line.
   */
  const wordsOf = (page: HTMLElement, el: HTMLElement) => {
    const origin = page.getBoundingClientRect()
    const mark = document.createElement('span')
    mark.style.cssText = 'display: inline-block; width: 0; height: 0'
    if (el instanceof HTMLInputElement) {
      const style = getComputedStyle(el)
      const start =
        el.getBoundingClientRect().left +
        Number.parseFloat(style.borderLeftWidth) +
        Number.parseFloat(style.paddingLeft) -
        el.scrollLeft
      const row = document.createElement('div')
      row.style.cssText =
        'position: absolute; inset: 0 0 auto; display: flex; align-items: baseline; visibility: hidden'
      const copy = el.cloneNode() as HTMLInputElement
      copy.removeAttribute('name')
      copy.value = el.value
      row.append(mark, copy)
      el.after(row)
      const below =
        mark.getBoundingClientRect().bottom - copy.getBoundingClientRect().top
      row.remove()
      return {
        start: start - origin.left,
        baseline: el.getBoundingClientRect().top + below - origin.top,
      }
    }
    const first = document.createRange()
    first.setStart(el.firstChild as Node, 0)
    first.setEnd(el.firstChild as Node, 1)
    el.prepend(mark)
    const baseline = mark.getBoundingClientRect().bottom
    mark.remove()
    return {
      start: first.getBoundingClientRect().left - origin.left,
      baseline: baseline - origin.top,
    }
  }

  /** Each thing that is gone, or moved or resized by over half a pixel. */
  const moved = (
    before: ReturnType<typeof measure>,
    after: ReturnType<typeof measure>,
  ) =>
    Object.keys(before)
      .filter((name) => {
        const [was, is] = [before[name], after[name]]
        return (
          was === null ||
          is === null ||
          was.some((value, i) => Math.abs(value - is[i]) > 0.5)
        )
      })
      .map(
        (name) =>
          `${name}: ${JSON.stringify(before[name])} → ${JSON.stringify(after[name])}`,
      )

  for (const { at, width, height, beside } of PLACES) {
    for (const [named, name] of [
      ['a name on one line', 'Manga'],
      ['a name that wraps', WRAPPED_NAME],
    ] as const) {
      for (const tall of [false, true]) {
        it(`moves nothing around the box while ${named} is edited${tall ? ' in a box taller than the title' : ''} and refused, at ${at}`, async () => {
          vi.mocked(librariesApi.listLibraries).mockResolvedValue({
            libraries: [NOVELS, { ...MANGA, name }],
          })
          vi.mocked(librariesApi.renameLibrary).mockRejectedValue(
            new Error(
              '/api/library/libraries/1: 400 {"error":"a library is already called `LN`"}',
            ),
          )
          // The library's page is at its name.
          const entry = `${TABLE}/${encodeURIComponent(name)}`
          const host = await shell(
            width,
            height,
            beside ? `${entry}/dirs/1` : entry,
          )
          const page = host.querySelector('.library-page') as HTMLElement
          // Beside a directory, the page is the pair's list column.
          const column = page.closest<HTMLElement>('.split-pair > .split-list')
          expect(column !== null).toBe(beside)
          if (column !== null) {
            expect(host.querySelector('.split-detail .dir-page')).not.toBeNull()
          }
          const title = page.querySelector(
            '.page-header .page-title',
          ) as HTMLElement
          expect(title.textContent).toBe(name)
          const face = (el: Element) => {
            const style = getComputedStyle(el)
            return [
              style.fontFamily,
              style.fontSize,
              style.fontWeight,
              style.lineHeight,
            ]
          }
          const titleFace = face(title)
          const line = Number.parseFloat(titleFace[3])
          const [titleLeft, , titleWidth, titleHeight] = from(page, title)
          // On one line or on more, as the case says.
          expect(titleHeight > 1.5 * line).toBe(name !== 'Manga')
          const before = measure(page)
          const titleWords = wordsOf(page, title)

          await click(
            page.querySelector('button[name="rename-library"]') as Element,
          )
          const box = nameBox(host) as HTMLInputElement
          expect(document.activeElement).toBe(box)
          if (tall) box.style.lineHeight = '2'
          const [boxLeft, , boxWidth, boxHeight] = from(page, box)
          // From the title's start to the end of its column, inside it, so
          // that a column that clips at its edge cuts none of it off.
          expect.soft(Math.abs(boxLeft - titleLeft)).toBeLessThanOrEqual(0.5)
          expect
            .soft(Math.abs(boxLeft + boxWidth - (titleLeft + titleWidth)))
            .toBeLessThanOrEqual(0.5)
          // The name's words where the title's were: the first letter at
          // the title's, on the title's baseline.
          const boxWords = wordsOf(page, box)
          expect
            .soft(Math.abs(boxWords.start - titleWords.start), 'words start')
            .toBeLessThanOrEqual(0.5)
          expect
            .soft(Math.abs(boxWords.baseline - titleWords.baseline), 'baseline')
            .toBeLessThanOrEqual(0.5)
          // Marked by a line under it and no other edge: the accent's while
          // it has focus, which is its focus mark, and no ring drawn. The
          // ring it keeps is transparent, for forced colours to draw.
          const edge = getComputedStyle(box)
          expect
            .soft([
              edge.borderTopStyle,
              edge.borderLeftStyle,
              edge.borderRightStyle,
            ])
            .toEqual(['none', 'none', 'none'])
          expect
            .soft([edge.outlineStyle, edge.outlineColor])
            .toEqual(['solid', 'rgba(0, 0, 0, 0)'])
          expect
            .soft([
              edge.borderBottomStyle,
              edge.borderBottomWidth,
              edge.borderBottomColor,
            ])
            .toEqual(['solid', '2px', tokenColour(page, '--accent')])
          if (tall) {
            expect.soft(boxHeight).toBeGreaterThan(line + 10)
          } else {
            expect.soft(face(box)).toEqual(titleFace)
          }
          expect.soft(moved(before, measure(page)), 'on opening').toEqual([])
          // Save · Cancel in the slot Rename… · Scan library keep: Save
          // where Rename… is, Cancel inside the slot's end.
          const [renameLeft, renameTop] = before['Rename…'] as number[]
          const [scanLeft, , scanWidth] = before['Scan library'] as number[]
          const save = from(page, saveName(host) as HTMLElement)
          const cancel = from(
            page,
            page.querySelector('button[name="cancel-rename"]') as Element,
          )
          expect.soft(Math.abs(save[0] - renameLeft)).toBeLessThanOrEqual(0.5)
          for (const [left, top, w] of [save, cancel]) {
            expect.soft(Math.abs(top - renameTop)).toBeLessThanOrEqual(0.5)
            expect
              .soft(left + w)
              .toBeLessThanOrEqual(scanLeft + scanWidth + 0.5)
          }
          if (column !== null) {
            expect.soft(column.scrollWidth).toBe(column.clientWidth)
          }
          const doc = document.documentElement
          expect.soft(doc.scrollWidth).toBeLessThanOrEqual(doc.clientWidth)

          await fill(box, 'LN')
          await click(saveName(host) as Element)
          await settle()
          const alert = pageRefusal(host) as HTMLElement
          expect(alert.textContent).toContain('is already called')
          const words = document.createRange()
          words.selectNodeContents(alert)
          expect(words.getClientRects()).toHaveLength(1)
          expect
            .soft(moved(before, measure(page)), 'on the refusal')
            .toEqual([])
          // Focus is on Save now: the line under the box is at rest.
          expect
            .soft(getComputedStyle(box).borderBottomColor, 'at rest')
            .toBe(tokenColour(page, '--line-strong'))
        })
      }
    }
  }

  /**
   * The name of a library the list has not brought, so nothing here says it
   * clashes until the server refuses it. It is long enough that the refusal
   * takes more than one line at each place and lies over the first of the
   * Directories' rows: one line lies over their heading alone, which is not
   * positioned, so it cannot show which of a row and the refusal is drawn
   * above.
   */
  const LONG_NAME =
    'Manga and light novels from the shelves in the back room, by publisher and then by series, with each series’ omnibus editions kept apart from the single volumes they collect, its colour reprints kept apart from the originals, and its artbooks, guidebooks and fan books kept apart from the story, so that a person who wants only the story finds only the story'

  /** The server's refusal of a rename to `LONG_NAME` (`libraries::rename`). */
  const LONG_REFUSAL = `a library is already called \`${LONG_NAME}\``

  /** `a` and `b`'s overlap, or `null` where they do not meet. */
  const overlap = (a: DOMRect, b: DOMRect) => {
    const left = Math.max(a.left, b.left)
    const right = Math.min(a.right, b.right)
    const top = Math.max(a.top, b.top)
    const bottom = Math.min(a.bottom, b.bottom)
    return right > left && bottom > top ? { left, right, top, bottom } : null
  }

  /** Where `box` lies over the first of the Directories' rows it meets. */
  const overFirstRow = (page: HTMLElement, box: DOMRect) => {
    for (const row of section(page, 'Directories').querySelectorAll(
      'tbody tr',
    )) {
      const met = overlap(box, row.getBoundingClientRect())
      if (met !== null) return met
    }
    return null
  }

  /**
   * Each point of a 5 × 5 grid over `area`, a pixel inside its edges, where
   * something other than `el` is found: what is drawn there, and what a
   * press there would reach. A row is positioned (`admin.css`) and its link
   * covers it, so a row drawn above `el` shows here as the row's link.
   */
  const coveredIn = (
    el: Element,
    area: { left: number; right: number; top: number; bottom: number },
  ) => {
    const covered: string[] = []
    for (let i = 0; i < 5; i++) {
      for (let j = 0; j < 5; j++) {
        const x = area.left + 1 + ((area.right - area.left - 2) * i) / 4
        const y = area.top + 1 + ((area.bottom - area.top - 2) * j) / 4
        const hit = document.elementFromPoint(x, y)
        if (!el.contains(hit)) {
          covered.push(
            `${Math.round(x)},${Math.round(y)}: ${hit?.tagName}.${hit?.className}`,
          )
        }
      }
    }
    return covered
  }

  for (const { at, width, height, beside } of PLACES) {
    it(`lays a refusal over what is under the header, inside the page’s column, in the look of the page’s other refusals, and moves nothing as the editor opens, as the refusal comes, as an edit takes it away or as Cancel closes the editor, at ${at}`, async () => {
      vi.mocked(librariesApi.renameLibrary).mockRejectedValue(
        new Error(
          `/api/library/libraries/1: 400 ${JSON.stringify({ error: LONG_REFUSAL })}`,
        ),
      )
      onTestFinished(() =>
        document.documentElement.removeAttribute('data-theme'),
      )
      const host = await shell(
        width,
        height,
        beside ? `${MANGA_PAGE}/dirs/1` : MANGA_PAGE,
      )
      const page = host.querySelector('.library-page') as HTMLElement
      expect(page.closest('.split-pair > .split-list') !== null).toBe(beside)
      const before = measure(page)
      const refuse = async () => {
        await click(saveName(host) as Element)
        await settle()
        const alert = pageRefusal(host) as HTMLElement
        expect(alert.textContent).toBe(LONG_REFUSAL)
        return alert
      }

      await click(
        page.querySelector('button[name="rename-library"]') as Element,
      )
      expect.soft(moved(before, measure(page)), 'on opening').toEqual([])
      const box = nameBox(host) as HTMLInputElement
      await fill(box, LONG_NAME)
      const alert = await refuse()
      expect.soft(moved(before, measure(page)), 'on the refusal').toEqual([])
      // Not in the count's place: the count stays as it was.
      expect
        .soft(
          page.querySelector('.page-header .page-description')?.textContent,
          'the count',
        )
        .toBe('1858 works in 2 directories')
      // More than one line: in the flow, the lines under the header would
      // move by each.
      const words = document.createRange()
      words.selectNodeContents(alert)
      const lines = new Set(
        [...words.getClientRects()].map((r) => Math.round(r.top)),
      )
      expect.soft(lines.size, 'lines').toBeGreaterThan(1)

      // Over the Directories' rows, and drawn above them where they meet.
      const callout = alert.getBoundingClientRect()
      const met = overFirstRow(page, callout)
      expect.soft(met, 'over a row of the Directories').not.toBeNull()
      if (met !== null) {
        expect.soft(coveredIn(alert, met), 'drawn above it').toEqual([])
      }
      // Inside the page's column, and clear of the phone bar.
      const header = (
        page.querySelector('.page-header') as HTMLElement
      ).getBoundingClientRect()
      expect
        .soft(callout.left, 'start')
        .toBeGreaterThanOrEqual(header.left - 0.5)
      expect.soft(callout.right, 'end').toBeLessThanOrEqual(header.right + 0.5)
      expect.soft(callout.top, 'top').toBeGreaterThanOrEqual(barBottom())
      const column = page.closest<HTMLElement>('.split-list')
      if (column !== null) {
        expect.soft(column.scrollWidth).toBe(column.clientWidth)
      }
      const doc = document.documentElement
      expect.soft(doc.scrollWidth).toBeLessThanOrEqual(doc.clientWidth)

      // A sentence the page draws as a refusal of its own, with nothing but
      // the page's styles on it, has the words' look every other refusal
      // here has.
      const other = document.createElement('p')
      other.setAttribute('role', 'alert')
      other.textContent = 'refused'
      page.append(other)
      const look = (el: Element) => {
        const style = getComputedStyle(el)
        return [
          style.fontFamily,
          style.fontSize,
          style.fontWeight,
          style.lineHeight,
          style.color,
        ]
      }
      expect.soft(look(alert), 'the words').toEqual(look(other))
      other.remove()
      // On the raised surface, off the page's ground, inside an edge, in
      // either theme.
      for (const theme of ['paper', 'ink']) {
        document.documentElement.setAttribute('data-theme', theme)
        const style = getComputedStyle(alert)
        const sides = ['Top', 'Right', 'Bottom', 'Left'] as const
        expect
          .soft(
            sides.map((side) => [
              style.getPropertyValue(`border-${side.toLowerCase()}-style`),
              style.getPropertyValue(`border-${side.toLowerCase()}-color`),
            ]),
            `${theme}: the edge`,
          )
          .toEqual(
            sides.map(() => ['solid', tokenColour(page, '--line-strong')]),
          )
        expect
          .soft(style.backgroundColor, `${theme}: the surface`)
          .toBe(tokenColour(page, '--surface'))
        expect
          .soft(style.backgroundColor, `${theme}: off the page’s ground`)
          .not.toBe(getComputedStyle(document.body).backgroundColor)
      }
      document.documentElement.removeAttribute('data-theme')

      // An edit takes it away, and moves nothing.
      await fill(box, 'Manga 3')
      expect.soft(pageRefusal(host), 'after an edit').toBeNull()
      expect.soft(moved(before, measure(page)), 'on the edit').toEqual([])
      // Cancel takes it away with the editor, and moves nothing.
      await fill(box, LONG_NAME)
      await refuse()
      await click(page.querySelector('button[name="cancel-rename"]') as Element)
      await settle()
      expect.soft(nameBox(host), 'the editor after Cancel').toBeNull()
      expect.soft(pageRefusal(host), 'after Cancel').toBeNull()
      expect.soft(moved(before, measure(page)), 'on Cancel').toEqual([])
    })
  }

  /** Why the name typed would be refused, said as it is typed. */
  const clashNote = (host: HTMLElement) =>
    host.querySelector<HTMLElement>(
      '.library-page .page-header-answer > [role="status"]',
    )

  for (const { at, width, height, beside } of PLACES) {
    it(`says a clash as it is typed, over the page in a refusal’s look, holds Save, and moves nothing as it comes or goes, at ${at}`, async () => {
      onTestFinished(() =>
        document.documentElement.removeAttribute('data-theme'),
      )
      const host = await shell(
        width,
        height,
        beside ? `${MANGA_PAGE}/dirs/1` : MANGA_PAGE,
      )
      const page = host.querySelector('.library-page') as HTMLElement
      expect(page.closest('.split-pair > .split-list') !== null).toBe(beside)
      const before = measure(page)

      await click(
        page.querySelector('button[name="rename-library"]') as Element,
      )
      const box = nameBox(host) as HTMLInputElement
      expect(document.activeElement).toBe(box)
      const note = clashNote(host) as HTMLElement
      // Drawn for as long as the box is, and while empty it takes no room,
      // so it lies over nothing.
      expect.soft(note.getBoundingClientRect().height, 'empty').toBe(0)
      expect.soft(moved(before, measure(page)), 'on opening').toEqual([])

      // Real keys, over the name the editor opens with selected.
      await keys('LIGHT NOVELS')
      expect(box.value).toBe('LIGHT NOVELS')
      expect(note.textContent).toBe(
        'a library is already called `Light novels`',
      )
      expect.soft(moved(before, measure(page)), 'on the clash').toEqual([])
      // Over the Directories.
      expect
        .soft(
          overlap(
            note.getBoundingClientRect(),
            section(page, 'Directories').getBoundingClientRect(),
          ),
          'over the Directories',
        )
        .not.toBeNull()
      // And drawn above their rows. One line lies over their heading alone,
      // so it is laid lower, over the first row, where a longer note, a long
      // name's, lies, and put back.
      const answer = note.parentElement as HTMLElement
      const first = section(page, 'Directories').querySelector(
        'tbody tr',
      ) as HTMLElement
      const lower =
        first.getBoundingClientRect().top - note.getBoundingClientRect().top
      answer.style.top = `calc(100% + ${lower}px)`
      const met = overFirstRow(page, note.getBoundingClientRect())
      expect.soft(met, 'laid over a row of the Directories').not.toBeNull()
      if (met !== null) {
        expect.soft(coveredIn(note, met), 'drawn above it').toEqual([])
      }
      answer.style.removeProperty('top')
      // In the look of a refusal laid over the page there, in either theme.
      const refusal = document.createElement('p')
      refusal.setAttribute('role', 'alert')
      refusal.textContent = 'refused'
      note.after(refusal)
      const look = (el: Element) => {
        const style = getComputedStyle(el)
        return [
          style.borderTopStyle,
          style.borderTopColor,
          style.borderLeftStyle,
          style.backgroundColor,
          style.boxShadow,
          style.paddingTop,
          style.paddingLeft,
          style.fontSize,
          style.color,
        ]
      }
      for (const theme of ['paper', 'ink']) {
        document.documentElement.setAttribute('data-theme', theme)
        expect.soft(look(note), theme).toEqual(look(refusal))
      }
      document.documentElement.removeAttribute('data-theme')
      refusal.remove()
      // Save is drawn held, by its dashed edge.
      const save = saveName(host) as HTMLButtonElement
      expect(save.disabled).toBe(true)
      expect
        .soft(getComputedStyle(save).borderTopStyle, 'Save held')
        .toBe('dashed')

      // Typing on takes it away, and moves nothing.
      await keys('{Backspace}')
      expect(note.textContent).toBe('')
      expect.soft(note.getBoundingClientRect().height, 'cleared').toBe(0)
      expect.soft(moved(before, measure(page)), 'on clearing').toEqual([])
      expect(save.disabled).toBe(false)
      expect
        .soft(getComputedStyle(save).borderTopStyle, 'Save free')
        .toBe('solid')
    })
  }

  /** Save and Cancel, drawn while the name is edited. */
  const saveAndCancel = (host: HTMLElement) =>
    ['save-rename', 'cancel-rename'].map(
      (name) =>
        host.querySelector(
          `.library-page button[name="${name}"]`,
        ) as HTMLButtonElement,
    )

  it('never draws Save waiting, nor Save or Cancel held, for a save answered at once, in any frame', async () => {
    vi.mocked(librariesApi.renameLibrary).mockResolvedValue({
      id: 1,
      name: 'Manga 2',
    } as never)
    listFollowingRename('Manga 2')
    const host = await shell(1280, 800, MANGA_PAGE)
    await click(host.querySelector('button[name="rename-library"]') as Element)
    await fill(nameBox(host) as Element, 'Manga 2')
    const [save, cancel] = saveAndCancel(host)
    // What each frame shows, and each state the DOM is put in between.
    const drawn = new Set<string>()
    const record = () =>
      drawn.add(`${save.textContent} ${save.disabled} ${cancel.disabled}`)
    const watch = new MutationObserver(record)
    for (const el of [save, cancel]) {
      watch.observe(el, {
        attributes: true,
        childList: true,
        characterData: true,
        subtree: true,
      })
    }
    let frames = 0
    let sampling = true
    const frame = () => {
      frames += 1
      record()
      if (sampling) requestAnimationFrame(frame)
    }
    requestAnimationFrame(frame)
    await click(save)
    await settle(WAIT_DRAWN_AFTER_MS + 100)
    sampling = false
    watch.disconnect()
    expect(nameBox(host)).toBeNull()
    expect(frames).toBeGreaterThan(5)
    expect([...drawn]).toEqual(['Save false false'])
  })

  for (const [width, height] of SIZES) {
    it(`draws a save still out after a while as waiting, in the width Save had, so that neither it nor Cancel moves, at ${width}`, async () => {
      const answer = deferred<{ id: number; name: string }>()
      vi.mocked(librariesApi.renameLibrary).mockReturnValue(
        answer.promise as never,
      )
      listFollowingRename('Manga 2')
      const host = await shell(width, height, MANGA_PAGE)
      await click(
        host.querySelector('button[name="rename-library"]') as Element,
      )
      await fill(nameBox(host) as Element, 'Manga 2')
      const [save, cancel] = saveAndCancel(host)
      const before = [boxOf(save), boxOf(cancel)]
      // Its name is its label's words alone (O175), whatever width it keeps.
      expect(
        page.getByRole('button', { name: 'Save', exact: true }).element(),
      ).toBe(save)

      // The wait's timer on a clock the test moves, so that the save is
      // read not yet waiting a millisecond before the wait is drawn, with no
      // race against the time the click itself takes.
      holdTheClock()
      await click(save)
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect([save.textContent, save.disabled, cancel.disabled]).toEqual([
        'Save',
        false,
        false,
      ])
      pass(1)
      expect([save.textContent, save.disabled, cancel.disabled]).toEqual([
        'Saving…',
        true,
        true,
      ])
      vi.useRealTimers()
      expectKept('Save', before[0], boxOf(save))
      expectKept('Cancel', before[1], boxOf(cancel))
      await act(async () => {
        answer.resolve({ id: 1, name: 'Manga 2' })
        await new Promise((r) => setTimeout(r, 20))
      })
      expect(nameBox(host)).toBeNull()
    })
  }

  it('hides the title and the buttons not in use, and takes none of them into the tab order', async () => {
    const host = await shell(1280, 800, MANGA_PAGE)
    const page = host.querySelector('.library-page') as HTMLElement
    const named = (name: string) =>
      page.querySelector(`button[name="${name}"]`) as HTMLElement
    const title = page.querySelector('.page-header .page-title') as HTMLElement
    const hidden = (els: HTMLElement[]) =>
      els.map((el) => getComputedStyle(el).visibility === 'hidden')

    // Closed: Save and Cancel keep their slot, unseen, and the keys go from
    // Scan library past them to the first directory.
    const idle = [named('save-rename'), named('cancel-rename')]
    expect(hidden(idle)).toEqual([true, true])
    await click(title)
    expect(document.activeElement).toBe(title)
    await keys('{Tab}')
    expect(document.activeElement).toBe(named('rename-library'))
    await keys('{Tab}')
    expect(document.activeElement).toBe(named('scan-library'))
    await keys('{Tab}')
    expect(document.activeElement).toBe(
      page.querySelector('.dir-table .row-link'),
    )
    for (const el of idle) {
      el.focus()
      expect(document.activeElement).not.toBe(el)
    }

    // Open: the title and Rename… · Scan library are unseen beneath the
    // box and Save · Cancel, and take no focus.
    await click(named('rename-library'))
    const box = nameBox(host) as HTMLInputElement
    expect(document.activeElement).toBe(box)
    const open = [title, named('rename-library'), named('scan-library')]
    expect(hidden(open)).toEqual([true, true, true])
    expect(hidden(idle)).toEqual([false, false])
    for (const el of open) {
      el.focus()
      expect(document.activeElement).toBe(box)
    }
    await keys('{Shift>}{Tab}{/Shift}')
    expect(document.activeElement).toBe(
      page.querySelector('.page-header .back a'),
    )
  })
})

/**
 * Move… opens a modal dialog, so the directory's page beneath it holds still
 * and is out of reach while it is open. Its box is looked up by what it
 * holds, Move to, so that what measures the page measures whatever form the
 * move's box takes.
 */
describe('Move… in its dialog, in the real Shell', () => {
  const moveButton = (host: HTMLElement) =>
    [...host.querySelectorAll<HTMLElement>('.dir-page button')].find(
      (b) => b.textContent === 'Move…',
    ) as HTMLElement
  /** Move to, drawn only while Move…'s box is open. */
  const moveTo = (host: HTMLElement) =>
    host.querySelector<HTMLSelectElement>('select[name="move-target"]')
  const moveBox = (host: HTMLElement) =>
    moveTo(host)?.closest('.move-root') as HTMLDialogElement
  const answer = (dry_run: boolean): librariesApi.MoveAnswer => ({
    moved: {
      library_id: 1,
      library: 'Manga',
      works_kept: 14,
      works_joined: 0,
      works_split: 0,
      series_joined: 0,
      positions: 3,
      notes: 1,
      alignments_dropped: 0,
      library_removed: 'Light novels',
      grants_removed: 0,
      left_with_no_library: [],
    },
    derive: emptyDerive,
    dry_run,
  })

  /** Opens Move… with a real mouse, chooses Manga and asks what that would do. */
  async function rehearse(host: HTMLElement) {
    vi.mocked(librariesApi.moveCost).mockResolvedValue(answer(true))
    await click(moveButton(host))
    await act(async () => {
      await userEvent.selectOptions(moveTo(host) as HTMLSelectElement, '1')
    })
    await click(host.querySelector('button[name="rehearse"]') as Element)
    await settle()
  }

  /**
   * The tops a person keeps their place by while the dialog is open: its
   * own, its title's and its first field's.
   */
  const tops = (host: HTMLElement) => {
    const box = moveBox(host)
    const title = document.getElementById(
      box.getAttribute('aria-labelledby') ?? '',
    ) as HTMLElement
    return {
      'the dialog': box.getBoundingClientRect().top,
      'its title': title.getBoundingClientRect().top,
      'Move to': (moveTo(host) as HTMLElement).getBoundingClientRect().top,
    }
  }
  const expectStill = (
    before: ReturnType<typeof tops>,
    after: ReturnType<typeof tops>,
    when: string,
  ) => {
    for (const [name, top] of Object.entries(before)) {
      expect
        .soft(
          Math.abs(after[name as keyof typeof after] - top),
          `${name}'s top ${when}`,
        )
        .toBeLessThanOrEqual(0.5)
    }
  }

  for (const [width, height] of SIZES) {
    it(`keeps How it is scanned where it was while Move… is open, the dialog’s top ${width === 1280 ? 'a step below the window’s and its sides centred' : 'at the page’s gutter and its sides inside it'}, and the page behind it inert, at ${width}`, async () => {
      const host = await shell(width, height, NOVELS_DIR_PAGE)
      const scanned = host.querySelector(
        '.dir-page > .settings-section',
      ) as HTMLElement
      expect(
        scanned.querySelector('.settings-section-title')?.textContent,
      ).toBe('How it is scanned')
      const before = scanned.getBoundingClientRect().top

      await click(moveButton(host))
      const target = moveTo(host) as HTMLSelectElement
      expect(target).not.toBeNull()
      expect(
        Math.abs(scanned.getBoundingClientRect().top - before),
      ).toBeLessThanOrEqual(0.5)
      expect(moveBox(host).matches(':modal')).toBe(true)
      // Its top pinned, at a desk's width `--space-7` from the window's and
      // on a phone at the page's gutter; across, centred at a desk's width.
      // Below 900px the document keeps a scrollbar's gutter (`settings.css`),
      // which the box is centred clear of, so there its sides are held
      // inside the page's gutter instead.
      const at = moveBox(host).getBoundingClientRect()
      const doc = document.documentElement
      const step = Number.parseFloat(
        getComputedStyle(doc).getPropertyValue(
          width === 1280 ? '--space-7' : '--space-4',
        ),
      )
      expect(step).toBe(width === 1280 ? 48 : 16)
      expect(Math.abs(at.top - step)).toBeLessThanOrEqual(0.5)
      if (width === 1280) {
        expect(
          Math.abs(at.left - (doc.clientWidth - at.right)),
        ).toBeLessThanOrEqual(1)
      } else {
        const gutter = Number.parseFloat(
          getComputedStyle(host.querySelector('main') as HTMLElement)
            .paddingLeft,
        )
        expect(gutter).toBe(16)
        expect(at.left).toBeGreaterThanOrEqual(gutter - 0.5)
        expect(at.right).toBeLessThanOrEqual(doc.clientWidth - gutter + 0.5)
      }
      expect(document.activeElement).toBe(target)
      // A box on the page behind is not disabled, and still takes no focus.
      ;(
        host.querySelector('.dir-page input[name="edition_key"]') as HTMLElement
      ).focus()
      expect(document.activeElement).toBe(target)
    })

    /**
     * What a rehearsal answers makes the dialog taller. A dialog centred in
     * the window would rise by half of it, and the title and the field being
     * read with it; the dialog grows downward instead.
     */
    it(`keeps the dialog’s top, its title’s and Move to’s where they were when the rehearsal’s answer arrives, at ${width}`, async () => {
      vi.mocked(librariesApi.moveCost).mockResolvedValue(answer(true))
      const host = await shell(width, height, NOVELS_DIR_PAGE)
      moveButton(host).focus()
      await click(moveButton(host))
      await act(async () => {
        await userEvent.selectOptions(moveTo(host) as HTMLSelectElement, '1')
      })
      const before = tops(host)

      await click(host.querySelector('button[name="rehearse"]') as Element)
      await settle()
      expect(moveBox(host).querySelector('.move-counts')).not.toBeNull()
      expectStill(before, tops(host), 'when the answer arrives')
    })

    it(`keeps the dialog’s top, its title’s and Move to’s where they were when What deriving after the move would find opens, at ${width}`, async () => {
      const host = await shell(width, height, NOVELS_DIR_PAGE)
      moveButton(host).focus()
      await rehearse(host)
      const derive = moveBox(host).querySelector(
        '.removal-derive',
      ) as HTMLDetailsElement
      const summary = derive.querySelector('summary') as HTMLElement
      expect(summary.textContent).toBe(
        'What deriving after the move would find',
      )
      // In sight before the press, so that the press scrolls nothing and
      // whatever moves is moved by the disclosure.
      summary.scrollIntoView({ block: 'nearest' })
      const before = tops(host)

      await click(summary)
      await settle()
      expect(derive.open).toBe(true)
      expectStill(before, tops(host), 'when it opens')
    })

    /**
     * The settings pages' rows: each field's label in its own case, beside
     * the field where the box is wide enough, and the buttons a row's gap
     * beneath the last field, each of them, whichever fields and buttons
     * are drawn. Found by their words, so that the measure is the same
     * whatever form the fields take.
     */
    it(`draws Move…’s fields under labels in their own case${width === 1280 ? ', beside them,' : ''} and each of its buttons a row’s gap beneath the last field, at ${width}`, async () => {
      vi.mocked(librariesApi.moveCost).mockResolvedValue(answer(true))
      const host = await shell(width, height, NOVELS_DIR_PAGE)
      await click(moveButton(host))
      const box = moveBox(host)
      const gap = Number.parseFloat(
        getComputedStyle(document.documentElement).getPropertyValue(
          '--space-3',
        ),
      )
      expect(gap).toBe(12)
      /** The element holding exactly a label's words. */
      const words = (label: string) =>
        [...box.querySelectorAll<HTMLElement>('*')].find(
          (el) => el.childElementCount === 0 && el.textContent === label,
        ) as HTMLElement
      const check = (fields: [string, HTMLElement][], buttons: string[]) => {
        for (const [label, field] of fields) {
          const at = words(label).getBoundingClientRect()
          const control = field.getBoundingClientRect()
          expect
            .soft(getComputedStyle(words(label)).textTransform, label)
            .toBe('none')
          if (width === 1280) {
            expect
              .soft(at.right, `${label} beside its field`)
              .toBeLessThan(control.left)
            expect
              .soft(at.top + at.height / 2, `${label} on its field's line`)
              .toBeGreaterThan(control.top)
            expect
              .soft(at.top + at.height / 2, `${label} on its field's line`)
              .toBeLessThan(control.bottom)
          }
        }
        const last = fields.at(-1)?.[1].getBoundingClientRect() as DOMRect
        const drawn = [...box.querySelectorAll<HTMLElement>('button')]
        expect(drawn.map((b) => b.textContent)).toEqual(buttons)
        for (const button of drawn) {
          expect
            .soft(
              button.getBoundingClientRect().top - last.bottom,
              `${button.textContent} under the last field`,
            )
            .toBeGreaterThanOrEqual(gap - 0.5)
        }
      }

      const target = moveTo(host) as HTMLSelectElement
      check([['Move to', target]], ['See what this would do', 'Cancel'])

      await act(async () => {
        await userEvent.selectOptions(target, 'new')
      })
      const named = box.querySelector(
        'input[name="new-library"]',
      ) as HTMLElement
      check(
        [
          ['Move to', target],
          ['Its name', named],
        ],
        ['See what this would do', 'Cancel'],
      )

      await act(async () => {
        await userEvent.selectOptions(target, '1')
      })
      await click(box.querySelector('button[name="rehearse"]') as Element)
      await settle()
      check([['Move to', target]], ['See what this would do', 'Move', 'Cancel'])
    })

    /**
     * What a rehearsal answers is read after See what this would do, which
     * asked for it, and before the buttons that act on it, as a removal's
     * counts are before its confirm: each of the refusal, the volumes in
     * its way, the counts, the accounts left with no library and what
     * deriving would find comes after the dry run and before every button
     * of the box's foot, in the document and on the screen. Each is a row
     * of the box as the fields are, the box's width, so the line above it
     * is too, and its sentences keep the measure prose keeps in a box.
     */
    const answered = async (
      host: HTMLElement,
      parts: string[],
      foot: string[],
    ) => {
      const inner = moveBox(host).querySelector(
        ':scope > .settings-box',
      ) as HTMLElement
      const style = getComputedStyle(inner)
      const edges = inner.getBoundingClientRect()
      const left =
        edges.left +
        Number.parseFloat(style.borderLeftWidth) +
        Number.parseFloat(style.paddingLeft)
      const right =
        edges.right -
        Number.parseFloat(style.borderRightWidth) -
        Number.parseFloat(style.paddingRight)
      const dryRun = inner.querySelector(
        'button[name="rehearse"]',
      ) as HTMLElement
      const drawn = [
        ...inner.querySelectorAll<HTMLElement>(
          ':scope > .settings-actions > button',
        ),
      ]
      expect(drawn.map((b) => b.textContent)).toEqual(foot)
      for (const selector of parts) {
        const part = inner.querySelector<HTMLElement>(selector)
        expect(part, selector).not.toBeNull()
        if (part === null) continue
        const at = part.getBoundingClientRect()
        const after = `${selector} after See what this would do`
        expect
          .soft(
            dryRun.compareDocumentPosition(part) &
              Node.DOCUMENT_POSITION_FOLLOWING,
            after,
          )
          .toBeTruthy()
        expect
          .soft(at.top, after)
          .toBeGreaterThanOrEqual(dryRun.getBoundingClientRect().bottom - 0.5)
        for (const button of drawn) {
          const named = `${selector} before ${button.textContent}`
          expect
            .soft(
              part.compareDocumentPosition(button) &
                Node.DOCUMENT_POSITION_FOLLOWING,
              named,
            )
            .toBeTruthy()
          expect
            .soft(at.bottom, named)
            .toBeLessThanOrEqual(button.getBoundingClientRect().top + 0.5)
        }
        expect
          .soft(
            [at.left - left, at.right - right].map((d) => Math.abs(d) <= 0.5),
            `${selector} the box's width`,
          )
          .toEqual([true, true])
      }
    }

    /** How far 60ch of `el`'s own face runs. */
    const measureOf = (el: HTMLElement) => {
      const probe = document.createElement('span')
      probe.style.cssText = 'display: inline-block; width: 60ch; height: 0'
      el.append(probe)
      const width = probe.getBoundingClientRect().width
      probe.remove()
      return width
    }

    /** Where `el`'s words end, from where its content box starts. */
    const wordsRun = (el: HTMLElement) => {
      const words = document.createRange()
      words.selectNodeContents(el)
      const start =
        el.getBoundingClientRect().left +
        Number.parseFloat(getComputedStyle(el).borderLeftWidth) +
        Number.parseFloat(getComputedStyle(el).paddingLeft)
      return (
        Math.max(...[...words.getClientRects()].map((line) => line.right)) -
        start
      )
    }

    it(`puts the counts, the accounts left with no library and what deriving would find between See what this would do and the move, as rows of its box, at ${width}`, async () => {
      vi.mocked(librariesApi.moveCost).mockResolvedValue({
        ...answer(true),
        moved: {
          ...answer(true).moved,
          left_with_no_library: ['ann', 'bob', 'carol', 'dave', 'erin'],
        },
      })
      const host = await shell(width, height, NOVELS_DIR_PAGE)
      await click(moveButton(host))
      await act(async () => {
        await userEvent.selectOptions(moveTo(host) as HTMLSelectElement, '1')
      })
      await click(host.querySelector('button[name="rehearse"]') as Element)
      await settle()
      await answered(
        host,
        ['.move-counts', '.left-with-no-library', '.removal-derive'],
        ['Move', 'Cancel'],
      )
      const box = moveBox(host)
      const counts = box.querySelector('.move-counts') as HTMLElement
      const gap = Number.parseFloat(
        getComputedStyle(document.documentElement).getPropertyValue(
          '--space-3',
        ),
      )
      // The counts' first sentence a row's padding beneath the row's line.
      const line =
        counts.getBoundingClientRect().top +
        Number.parseFloat(getComputedStyle(counts).borderTopWidth)
      expect
        .soft(
          (counts.firstElementChild as HTMLElement).getBoundingClientRect()
            .top - line,
        )
        .toBeCloseTo(gap, 0)
      for (const prose of [
        ...counts.querySelectorAll<HTMLElement>(':scope > p'),
        box.querySelector('.left-with-no-library') as HTMLElement,
      ]) {
        expect
          .soft(wordsRun(prose), prose.textContent?.slice(0, 40))
          .toBeLessThanOrEqual(measureOf(prose) + 0.5)
      }
    })

    it(`puts a refusal and the volumes in its way between See what this would do and Cancel, as rows of its box, at ${width}`, async () => {
      vi.mocked(librariesApi.moveCost).mockRejectedValue(
        new Error(
          '/api/library/roots/3/move?dry_run=true: 409 {"error":"moving this directory would split a volume whose readers have places in it","volumes":["Spice and Wolf v01","Spice and Wolf v02"]}',
        ),
      )
      const host = await shell(width, height, NOVELS_DIR_PAGE)
      await click(moveButton(host))
      await act(async () => {
        await userEvent.selectOptions(moveTo(host) as HTMLSelectElement, '1')
      })
      await click(host.querySelector('button[name="rehearse"]') as Element)
      await settle()
      const volumes = moveBox(host).querySelector(
        '.move-volumes',
      ) as HTMLDetailsElement
      volumes.open = true
      await settle()
      await answered(host, ['[role="alert"]', '.move-volumes'], ['Cancel'])
      expect(volumes.querySelectorAll('li')).toHaveLength(2)
    })

    /**
     * See what this would do under the fields it rehearses, where their
     * boxes start, whichever fields are drawn and once it has answered; and
     * the box's foot holding the move and Cancel alone, on one line. On a
     * real library's directory moved to a new library, a move whose buttons
     * can spread over two lines.
     */
    it(`puts See what this would do under the last field, where the boxes start, and the move and Cancel alone on one line at the foot, at ${width}`, async () => {
      vi.mocked(librariesApi.listLibraries).mockResolvedValue({
        libraries: [
          { ...NOVELS, dirs: [dir(3, '/home/davey/temp/manga-test/Volumes')] },
          MANGA,
        ],
      })
      vi.mocked(librariesApi.moveCost).mockResolvedValue({
        ...answer(true),
        moved: { ...answer(true).moved, library_id: 9, library: 'volumes3' },
      })
      const host = await shell(width, height, NOVELS_DIR_PAGE)
      await click(moveButton(host))
      const box = moveBox(host).querySelector(
        ':scope > .settings-box',
      ) as HTMLElement
      const dryRun = box.querySelector('button[name="rehearse"]') as HTMLElement
      /** The dry run in the row after `field`'s, under it, at its start. */
      const underTheLast = (field: HTMLElement, drawn: string) => {
        const row = field.closest('.settings-row') as HTMLElement
        const at = dryRun.getBoundingClientRect()
        const control = field.getBoundingClientRect()
        expect
          .soft(row.nextElementSibling?.contains(dryRun), `${drawn}: next row`)
          .toBe(true)
        expect
          .soft(at.top, `${drawn}: under the last field`)
          .toBeGreaterThan(control.bottom)
        expect
          .soft(Math.abs(at.left - control.left), `${drawn}: at its start`)
          .toBeLessThanOrEqual(0.5)
      }

      const target = moveTo(host) as HTMLSelectElement
      underTheLast(target, 'Move to alone')
      await act(async () => {
        await userEvent.selectOptions(target, 'new')
      })
      const named = box.querySelector(
        'input[name="new-library"]',
      ) as HTMLInputElement
      underTheLast(named, 'Its name')
      await fill(named, 'volumes3')
      await click(dryRun)
      await settle()
      expect(box.querySelector('.move-counts')).not.toBeNull()
      underTheLast(named, 'answered')

      const foot = box.lastElementChild as HTMLElement
      expect(foot.classList.contains('settings-actions')).toBe(true)
      const buttons = [...foot.querySelectorAll<HTMLElement>(':scope > button')]
      expect.soft(buttons.map((b) => b.textContent)).toEqual(['Move', 'Cancel'])
      const buttonTops = buttons.map((b) => b.getBoundingClientRect().top)
      expect
        .soft(
          Math.max(...buttonTops) - Math.min(...buttonTops),
          'the foot on one line',
        )
        .toBeLessThanOrEqual(0.5)
    })
  }

  it('keeps See what this would do and its wait inside the box at 320, at the last field’s start, in one box at both labels', async () => {
    const asked = deferred<librariesApi.MoveAnswer>()
    vi.mocked(librariesApi.moveCost).mockReturnValue(asked.promise)
    const host = await shell(320, 700, NOVELS_DIR_PAGE)
    await click(moveButton(host))
    await act(async () => {
      await userEvent.selectOptions(moveTo(host) as HTMLSelectElement, '1')
    })
    const box = moveBox(host).querySelector(
      ':scope > .settings-box',
    ) as HTMLElement
    const dryRun = box.querySelector('button[name="rehearse"]') as HTMLElement
    const inside = (label: string) => {
      expect(dryRun.textContent).toBe(label)
      const style = getComputedStyle(box)
      const edges = box.getBoundingClientRect()
      const right =
        edges.right -
        Number.parseFloat(style.borderRightWidth) -
        Number.parseFloat(style.paddingRight)
      for (const el of [dryRun, dryRun.parentElement as HTMLElement]) {
        expect
          .soft(
            el.getBoundingClientRect().right,
            `${label}: ${el.className || el.tagName}`,
          )
          .toBeLessThanOrEqual(right + 0.5)
      }
      expect(
        Math.abs(
          dryRun.getBoundingClientRect().left -
            (moveTo(host) as HTMLElement).getBoundingClientRect().left,
        ),
        `${label}: at the last field’s start`,
      ).toBeLessThanOrEqual(0.5)
      const dialog = moveBox(host)
      expect(dialog.scrollWidth).toBeLessThanOrEqual(dialog.clientWidth)
    }
    inside('See what this would do')
    const before = boxOf(dryRun)
    await click(dryRun)
    await settle(WAIT_DRAWN_AFTER_MS + 100)
    inside('Working out what this would do…')
    expectKept('See what this would do', before, boxOf(dryRun))
    await act(async () => {
      asked.resolve(answer(true))
      await new Promise((r) => setTimeout(r, 20))
    })
  })

  /** `top` laid over the opaque `under`, as the window draws it. */
  const over = (top: Channels, under: Channels): Channels => ({
    r: top.r * top.a + under.r * (1 - top.a),
    g: top.g * top.a + under.g * (1 - top.a),
    b: top.b * top.a + under.b * (1 - top.a),
    a: 1,
  })

  /**
   * A directory's path with no space in it and no name long enough to need a
   * break inside it: it may break only after a / or a -.
   *
   * Its widest stretch with no - in it, the names from the last hex group to
   * `volume-`, is wider than every line a case below draws it on, and each
   * case asserts that first. Drawn without its breaks, the path must then be
   * broken inside that stretch, wherever a line runs out, which a case sees
   * unless a line happens to run out just after a /.
   */
  const SPACELESS =
    '/srv/stacks-2000/-data-media-archives-library/a915c27e-d4b1-4c02-9e7f-6b1a03c4d8e2/collection/libs/originals/2019/japanese/scans/tankobon/volume-extras'

  /**
   * The stretches of `path` a browser cannot break inside without
   * `PathWithBreaks`: it breaks a line after a `-` and not after a `/`.
   */
  const unbrokenRunsOf = (path: string) => path.split(/(?<=-)/)

  /**
   * The width of the widest of `runs`, each set on one line in `el`'s own
   * font: a span inside `el` inherits it, and is taken out again before
   * anything reads `el`'s text.
   */
  const widestIn = (el: Element, runs: string[]) => {
    const probe = document.createElement('span')
    probe.style.position = 'absolute'
    probe.style.whiteSpace = 'pre'
    probe.style.visibility = 'hidden'
    el.append(probe)
    const widths = runs.map((run) => {
      probe.textContent = run
      return probe.getBoundingClientRect().width
    })
    probe.remove()
    return Math.max(...widths)
  }

  /** How wide a line `block` lays out is: its content box. */
  const lineOf = (block: Element) => {
    const style = getComputedStyle(block)
    return (
      block.clientWidth -
      parseFloat(style.paddingLeft) -
      parseFloat(style.paddingRight)
    )
  }

  /**
   * What each case below rests on: the fixture's widest stretch with no `-`
   * is wider than the line `path` is drawn on, so a path drawn without its
   * breaks has to be broken where that line runs out. A layout that gives
   * the path a wider line fails here, not by passing against its defect.
   */
  const expectRunWiderThanLine = (path: Element, block: Element) => {
    const run = widestIn(path, unbrokenRunsOf(SPACELESS))
    expect(
      run,
      'the widest stretch with no - against the line it is drawn on',
    ).toBeGreaterThan(lineOf(block))
  }

  /**
   * Each place `el`'s words go onto another line, as the character before
   * the break, a bar, and the character after it.
   */
  const breaksIn = (el: Element) => {
    const drawn: { char: string; top: number }[] = []
    const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT)
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const text = node.textContent ?? ''
      for (let i = 0; i < text.length; i++) {
        const one = document.createRange()
        one.setStart(node, i)
        one.setEnd(node, i + 1)
        const at = one.getClientRects()[0]
        if (at !== undefined) drawn.push({ char: text[i], top: at.top })
      }
    }
    const breaks: string[] = []
    for (let i = 1; i < drawn.length; i++) {
      if (drawn[i].top > drawn[i - 1].top + 1) {
        breaks.push(`${drawn[i - 1].char}|${drawn[i].char}`)
      }
    }
    return breaks
  }

  for (const [width, height, beside] of [
    [1920, 900, 'beside its library'],
    [390, 844, 'alone'],
  ] as const) {
    it(`breaks a path with no space in the directory’s title, drawn ${beside}, and in Move…’s, only after a / or a -, at ${width}`, async () => {
      vi.mocked(librariesApi.listLibraries).mockResolvedValue({
        libraries: [
          NOVELS,
          { ...MANGA, dirs: [dir(1, SPACELESS), ...MANGA.dirs.slice(1)] },
        ],
      })
      const host = await shell(width, height, `${MANGA_PAGE}/dirs/1`)
      const title = host.querySelector('.dir-page .page-title') as HTMLElement
      expect(title.textContent).toBe(SPACELESS)
      expect(title.closest('.split-pair') !== null).toBe(width === 1920)
      expectRunWiderThanLine(title.querySelector('code') as Element, title)
      const titleBreaks = breaksIn(title)
      expect(titleBreaks.length).toBeGreaterThan(0)
      expect(titleBreaks.filter((at) => !/^[/-]\|/.test(at))).toEqual([])

      await click(moveButton(host))
      const dialog = moveBox(host)
      const heading = document.getElementById(
        dialog.getAttribute('aria-labelledby') ?? '',
      ) as HTMLElement
      expect(heading.textContent).toBe(`Move ${SPACELESS}`)
      expectRunWiderThanLine(heading.querySelector('code') as Element, heading)
      const headingBreaks = breaksIn(heading)
      expect(headingBreaks.length).toBeGreaterThan(0)
      // A space, between Move and the path, is a break between words.
      expect(headingBreaks.filter((at) => !/^[/ -]\|/.test(at))).toEqual([])
    })
  }

  // The table draws each path in two places: as the link to its directory,
  // and bare while the library's removal is open and the links are taken away.
  for (const [drawn, removing] of [
    ['as its link', false],
    ['bare, while the library’s removal is open', true],
  ] as const) {
    it(`breaks a path with no space in a library’s Directories, drawn ${drawn}, only after a / or a -, at 390`, async () => {
      vi.mocked(librariesApi.listLibraries).mockResolvedValue({
        libraries: [
          NOVELS,
          { ...MANGA, dirs: [dir(1, SPACELESS), ...MANGA.dirs.slice(1)] },
        ],
      })
      vi.mocked(librariesApi.libraryRemovalCost).mockResolvedValue({
        removed: { works: 0, positions: 0, notes: 0, left_with_no_library: [] },
        derive: emptyDerive,
        dry_run: true,
      })
      const host = await shell(390, 844, MANGA_PAGE)
      if (removing) {
        await press(
          host.querySelector('button[name="remove-library"]') as Element,
        )
      }
      const path = host.querySelector(
        '.library-page .dir-table tbody td.lead code',
      ) as HTMLElement
      expect(path.textContent).toBe(SPACELESS)
      expect(path.closest('a') === null).toBe(removing)
      expectRunWiderThanLine(path, path.closest('td') as Element)
      const breaks = breaksIn(path)
      expect(breaks.length).toBeGreaterThan(0)
      expect(breaks.filter((at) => !/^[/-]\|/.test(at))).toEqual([])
    })
  }

  for (const [theme, name] of [
    ['ink', 'Ink'],
    ['paper', 'Paper'],
  ] as const) {
    it(`lays Move…’s dialog on ${theme === 'ink' ? 'a ground lighter than the page’s, standing off the page its backdrop darkens' : 'the page’s own ground'}, in ${name}`, async () => {
      document.documentElement.setAttribute('data-theme', theme)
      onTestFinished(() =>
        document.documentElement.removeAttribute('data-theme'),
      )
      const host = await shell(1280, 800, NOVELS_DIR_PAGE)
      await click(moveButton(host))
      const dialog = moveBox(host)
      expect(dialog.open).toBe(true)
      const page = channels(getComputedStyle(document.body).backgroundColor)
      const darkened = over(
        channels(getComputedStyle(dialog, '::backdrop').backgroundColor),
        page,
      )
      const ground = channels(getComputedStyle(dialog).backgroundColor)
      if (theme === 'ink') {
        // Raised, as a panel over the page is, and as far off the darkened
        // page as the eye needs to see an edge without the border, which the
        // page's own ground, the dialog's in Paper, is too near it to give.
        expect(luminance(ground)).toBeGreaterThan(luminance(page))
        expect(contrast(ground, darkened)).toBeGreaterThanOrEqual(1.3)
      } else {
        expect(ground).toEqual(page)
      }
    })
  }

  for (const [theme, name] of [
    ['ink', 'Ink'],
    ['paper', 'Paper'],
  ] as const) {
    it(`fills Move…’s select and buttons apart from its dialog’s ground, and a held button with that ground, in ${name}`, async () => {
      document.documentElement.setAttribute('data-theme', theme)
      onTestFinished(() =>
        document.documentElement.removeAttribute('data-theme'),
      )
      const host = await shell(1920, 900, NOVELS_DIR_PAGE)
      await click(moveButton(host))
      const dialog = moveBox(host)
      const ground = getComputedStyle(dialog).backgroundColor
      const fill = (el: Element) => getComputedStyle(el).backgroundColor
      const controls = [
        ...dialog.querySelectorAll<HTMLButtonElement | HTMLSelectElement>(
          'select, button',
        ),
      ]
      const held = controls.filter((el) => holdOf(el) !== 'free')
      const live = controls.filter((el) => holdOf(el) === 'free')
      // No library chosen yet, so See what this would do is held.
      expect(held).toHaveLength(1)
      expect(held[0].textContent).toContain('See what this would do')
      expect(live.length).toBeGreaterThan(1)
      expect
        .soft(
          live
            .filter((el) => fill(el) === ground)
            .map((el) => el.getAttribute('name') ?? el.textContent),
          'filled as the ground',
        )
        .toEqual([])
      expect.soft(held.map(fill), 'held').toEqual([ground])
    })
  }

  it('closes Move…’s dialog on Escape and on Cancel, with focus back on Move… each time', async () => {
    const host = await shell(1280, 800, NOVELS_DIR_PAGE)
    await click(moveButton(host))
    expect(document.activeElement).toBe(moveTo(host))
    await keys('{Escape}')
    expect(moveTo(host)).toBeNull()
    expect(document.activeElement).toBe(moveButton(host))

    await click(moveButton(host))
    const cancel = [
      ...moveBox(host).querySelectorAll<HTMLElement>('button'),
    ].find((b) => b.textContent === 'Cancel') as HTMLElement
    await click(cancel)
    expect(moveTo(host)).toBeNull()
    expect(document.activeElement).toBe(moveButton(host))
  })

  it('keeps Move…’s dialog open on Escape while its move runs, and when anything else closes it then', async () => {
    vi.mocked(librariesApi.moveDir).mockReturnValue(new Promise(() => {}))
    const host = await shell(1280, 800, NOVELS_DIR_PAGE)
    await rehearse(host)
    const box = moveBox(host)
    const confirm = box.querySelector('.confirm') as HTMLElement
    await click(confirm)
    // Held from the send, whether or not its wait is drawn yet.
    expect(librariesApi.moveDir).toHaveBeenCalledTimes(1)
    let closed = 0
    box.addEventListener('close', () => {
      closed += 1
    })
    // Three times: a page may turn down only so many requests to close it
    // with no press between them, and Chromium lets the third close the
    // dialog however its `cancel` is answered.
    await keys('{Escape}')
    await keys('{Escape}')
    await keys('{Escape}')
    await settle(20)
    expect(closed).toBe(0)
    expect(moveBox(host)).toBe(box)
    expect(box.matches(':modal')).toBe(true)

    await act(async () => {
      box.close()
      await new Promise((r) => setTimeout(r, 20))
    })
    expect(box.matches(':modal')).toBe(true)
  })

  it('closes Move…’s dialog on Escape again once its move is refused, with focus back on Move…', async () => {
    const sent = deferred<librariesApi.MoveAnswer>()
    vi.mocked(librariesApi.moveDir).mockReturnValue(sent.promise)
    const host = await shell(1280, 800, NOVELS_DIR_PAGE)
    await rehearse(host)
    const box = moveBox(host)
    const confirm = box.querySelector('.confirm') as HTMLElement
    await click(confirm)
    await settle(WAIT_DRAWN_AFTER_MS + 100)
    expect(confirm.textContent).toBe('Moving…')
    await act(async () => {
      sent.reject(
        new Error(
          '/api/library/roots/3/move: 409 {"error":"a scan or a rule save is running; move the directory when it has finished","volumes":[]}',
        ),
      )
      await new Promise((r) => setTimeout(r, 20))
    })
    expect(box.querySelector('[role="alert"]')?.textContent).toContain(
      'is running',
    )
    await keys('{Escape}')
    expect(moveTo(host)).toBeNull()
    expect(document.activeElement).toBe(moveButton(host))
  })

  it('keeps Move…’s dialog inside the page’s gutter at 390, a long path broken inside it and its height inside the window, scrolling inside itself, and nothing scrolling sideways', async () => {
    const long = `/mnt/user/media 1/Light Novels/${'Volumes'.repeat(12)}`
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [{ ...NOVELS, dirs: [dir(3, long)] }, MANGA],
    })
    const host = await shell(390, 600, NOVELS_DIR_PAGE)
    const doc = document.documentElement
    const pageHeight = doc.scrollHeight
    await rehearse(host)
    const box = moveBox(host)
    ;(box.querySelector('.removal-derive') as HTMLDetailsElement).open = true
    await settle()

    const at = box.getBoundingClientRect()
    const gutter = Number.parseFloat(
      getComputedStyle(host.querySelector('main') as HTMLElement).paddingLeft,
    )
    expect(gutter).toBe(16)
    expect(at.left).toBeGreaterThanOrEqual(gutter - 0.5)
    expect(at.right).toBeLessThanOrEqual(doc.clientWidth - gutter + 0.5)
    expect(at.top).toBeGreaterThanOrEqual(gutter - 0.5)
    expect(at.bottom).toBeLessThanOrEqual(window.innerHeight - gutter + 0.5)
    // Taller than the window, and scrolled inside itself: the page behind
    // does not grow by it.
    expect(box.scrollHeight).toBeGreaterThan(box.clientHeight)
    expect(getComputedStyle(box).overflowY).toBe('auto')
    expect(doc.scrollHeight).toBe(pageHeight)

    const title = document.getElementById(
      box.getAttribute('aria-labelledby') ?? '',
    ) as HTMLElement
    expect(title.textContent).toBe(`Move ${long}`)
    expect(title.scrollWidth).toBeLessThanOrEqual(title.clientWidth)
    expect(
      [...box.querySelectorAll('*')]
        .filter((el) => el.getBoundingClientRect().right > at.right + 0.5)
        .map((el) => `${el.tagName.toLowerCase()}.${el.className}`),
    ).toEqual([])
    expect(box.scrollWidth).toBeLessThanOrEqual(box.clientWidth)
    expect(doc.scrollWidth).toBeLessThanOrEqual(doc.clientWidth)
  })

  it('grows Move…’s dialog down to the page’s gutter above the window’s foot at 1280, its top where it opened, and then scrolls inside itself', async () => {
    const host = await shell(1280, 500, NOVELS_DIR_PAGE)
    const doc = document.documentElement
    const pageHeight = doc.scrollHeight
    moveButton(host).focus()
    await rehearse(host)
    const box = moveBox(host)
    ;(box.querySelector('.removal-derive') as HTMLDetailsElement).open = true
    await settle()

    const at = box.getBoundingClientRect()
    const step = (name: string) =>
      Number.parseFloat(getComputedStyle(doc).getPropertyValue(name))
    expect(Math.abs(at.top - step('--space-7'))).toBeLessThanOrEqual(0.5)
    expect(
      Math.abs(at.bottom - (window.innerHeight - step('--space-4'))),
    ).toBeLessThanOrEqual(0.5)
    expect(box.scrollHeight).toBeGreaterThan(box.clientHeight)
    expect(doc.scrollHeight).toBe(pageHeight)
  })
})

describe('a library’s page in a real browser, when a list read holds Save', () => {
  it('moves focus from Save to the name box, where Escape still closes the editor', async () => {
    await page.viewport(1280, 800)
    const drawn = (others: librariesApi.Library[]) => (
      <MemoryRouter initialEntries={[MANGA_PAGE]}>
        <LibraryPage
          library={MANGA}
          libraries={[MANGA, ...others]}
          onAdded={() => undefined}
          onChanged={() => undefined}
          onRemoved={() => undefined}
        />
      </MemoryRouter>
    )
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(drawn([NOVELS]))
    })
    if (host === undefined) throw new Error('nothing mounted')
    await click(host.querySelector('button[name="rename-library"]') as Element)
    const box = nameBox(host) as HTMLInputElement
    expect(document.activeElement).toBe(box)
    // Real keys: the name, over the one the editor opens with selected, then
    // Tab to Save.
    await keys('Comics{Tab}')
    const save = saveName(host) as HTMLButtonElement
    expect(document.activeElement).toBe(save)

    // Another admin took the name; a scan's end reads the list again.
    await act(async () => {
      rerender(drawn([NOVELS, { ...NOVELS, id: 3, name: 'Comics' }]))
    })
    await settle()
    expect(save.disabled).toBe(true)
    expect(document.activeElement).toBe(box)
    await keys('{Escape}')
    expect(nameBox(host)).toBeNull()
  })
})

describe('a library’s page in a real browser, when a slow save holds Save', () => {
  it('moves focus from Save to the name box as the wait is drawn, and keeps it there through a refusal, where Escape still closes the editor', async () => {
    await page.viewport(1280, 800)
    const sent = deferred<{ id: number; name: string }>()
    vi.mocked(librariesApi.renameLibrary).mockReturnValue(sent.promise)
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <MemoryRouter initialEntries={[MANGA_PAGE]}>
          <LibraryPage
            library={MANGA}
            libraries={[MANGA, NOVELS]}
            onAdded={() => undefined}
            onChanged={() => undefined}
            onRemoved={() => undefined}
          />
        </MemoryRouter>,
      )
    })
    if (host === undefined) throw new Error('nothing mounted')
    await click(host.querySelector('button[name="rename-library"]') as Element)
    const box = nameBox(host) as HTMLInputElement
    expect(document.activeElement).toBe(box)
    // Real keys: the name, over the one the editor opens with selected, then
    // Tab to Save and Enter on it.
    await keys('Comics{Tab}')
    const save = saveName(host) as HTMLButtonElement
    expect(document.activeElement).toBe(save)
    await keys('{Enter}')
    expect(librariesApi.renameLibrary).toHaveBeenCalledWith(1, 'Comics')

    await settle(WAIT_DRAWN_AFTER_MS + 200)
    expect([save.disabled, save.textContent]).toEqual([true, 'Saving…'])
    expect(document.activeElement).toBe(box)

    await act(async () => {
      sent.reject(
        new Error(
          '/api/library/libraries/1: 400 {"error":"a library is already called `Comics`"}',
        ),
      )
    })
    await settle()
    expect(pageRefusal(host)?.textContent).toBe(
      'a library is already called `Comics`',
    )
    expect(document.activeElement).toBe(box)
    await keys('{Escape}')
    expect(nameBox(host)).toBeNull()
  })
})

describe('a library’s page in a real browser, when a slow save holds Cancel', () => {
  it('moves focus from Cancel, tabbed to inside the wait’s delay, to the name box as the wait is drawn, where Escape still closes the editor once the save is refused', async () => {
    await page.viewport(1280, 800)
    const sent = deferred<{ id: number; name: string }>()
    vi.mocked(librariesApi.renameLibrary).mockReturnValue(sent.promise)
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <MemoryRouter initialEntries={[MANGA_PAGE]}>
          <LibraryPage
            library={MANGA}
            libraries={[MANGA, NOVELS]}
            onAdded={() => undefined}
            onChanged={() => undefined}
            onRemoved={() => undefined}
          />
        </MemoryRouter>,
      )
    })
    if (host === undefined) throw new Error('nothing mounted')
    await click(host.querySelector('button[name="rename-library"]') as Element)
    const box = nameBox(host) as HTMLInputElement
    expect(document.activeElement).toBe(box)
    // Real keys: the name, Enter in the box to save, then Tab past Save to
    // Cancel before the wait is drawn.
    await keys('Comics{Enter}{Tab}{Tab}')
    expect(librariesApi.renameLibrary).toHaveBeenCalledWith(1, 'Comics')
    const cancel = host.querySelector(
      'button[name="cancel-rename"]',
    ) as HTMLButtonElement
    expect(document.activeElement).toBe(cancel)
    expect(cancel.disabled).toBe(false)

    await settle(WAIT_DRAWN_AFTER_MS + 200)
    expect(cancel.disabled).toBe(true)
    expect(document.activeElement).toBe(box)

    await act(async () => {
      sent.reject(
        new Error(
          '/api/library/libraries/1: 400 {"error":"a library is already called `Comics`"}',
        ),
      )
    })
    await settle()
    expect(document.activeElement).toBe(box)
    await keys('{Escape}')
    expect(nameBox(host)).toBeNull()
  })
})

describe('a library’s page in a real browser, when an add holds Add directory', () => {
  it('moves focus from Add directory to the path box, and keeps it there through a refusal', async () => {
    await page.viewport(1280, 800)
    const sent = deferred<config.Added>()
    vi.mocked(librariesApi.addDir).mockReturnValue(sent.promise)
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <MemoryRouter initialEntries={[MANGA_PAGE]}>
          <LibraryPage
            library={MANGA}
            libraries={[MANGA, NOVELS]}
            onAdded={() => undefined}
            onChanged={() => undefined}
            onRemoved={() => undefined}
          />
        </MemoryRouter>,
      )
    })
    if (host === undefined) throw new Error('nothing mounted')
    await click(host.querySelector('button[name="add-dir"]') as Element)
    const box = host.querySelector(
      '.add-root input[name="path"]',
    ) as HTMLInputElement
    expect(document.activeElement).toBe(box)
    // Real keys: the path, then Tab to Add directory and Enter on it.
    await keys('/media/x{Tab}')
    const pick = host.querySelector(
      '.add-root button[name="pick"]',
    ) as HTMLButtonElement
    expect(document.activeElement).toBe(pick)
    await keys('{Enter}')
    await settle()
    expect(librariesApi.addDir).toHaveBeenCalledWith(1, '/media/x')
    expect(pick.disabled).toBe(true)
    expect(document.activeElement).toBe(box)

    await act(async () => {
      sent.reject(
        new Error(
          '/api/library/libraries/1/dirs: 400 {"error":"/media/x: No such file or directory (os error 2)"}',
        ),
      )
    })
    await settle()
    expect(host.querySelector('.add-root [role="alert"]')?.textContent).toBe(
      '/media/x: No such file or directory (os error 2)',
    )
    expect(box.value).toBe('/media/x')
    expect(document.activeElement).toBe(box)
  })
})
