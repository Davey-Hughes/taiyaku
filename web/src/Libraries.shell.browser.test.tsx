import { act } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { page, userEvent } from 'vitest/browser'
// **Required**, the same reason `Config.browser.test.tsx` gives.
import './index.css'
import { Config } from './Config'
import * as config from './config'
import * as curation from './curation'
import { GateContext } from './gate'
import * as librariesApi from './libraries'
import * as library from './library'
import { Shell } from './Shell'
import { deferred } from './testing/async'
import { emptyDerive } from './testing/derive'
import { press, submit, typeInto } from './testing/input'
import { mount } from './testing/mount'
import * as usersApi from './users'

/**
 * The Libraries screen inside the real `Shell`. Below 900px the document
 * scrolls under a sticky phone bar and above it `<main>` scrolls, and what a
 * person can see of a report, a refusal or a failure depends on both: a test
 * shell without the bar cannot tell where the bar covers.
 */

vi.mock('./library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./library')>()),
  status: vi.fn(),
}))
vi.mock('./curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./curation')>()),
  listProposals: vi.fn(),
}))
vi.mock('./config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./config')>()),
  patchRoot: vi.fn(),
  removalCost: vi.fn(),
  removeRoot: vi.fn(),
  scanStatus: vi.fn(),
  startScan: vi.fn(),
  listRoots: vi.fn(),
}))
vi.mock('./libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./libraries')>()),
  addFolder: vi.fn(),
  createLibrary: vi.fn(),
  libraryRemovalCost: vi.fn(),
  libraryScanStatus: vi.fn(),
  listFolders: vi.fn(),
  listLibraries: vi.fn(),
  moveCost: vi.fn(),
  moveFolder: vi.fn(),
  removeLibrary: vi.fn(),
  renameLibrary: vi.fn(),
  startLibraryScan: vi.fn(),
}))
vi.mock('./users', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./users')>()),
  listUsers: vi.fn(),
}))
// The build's label at the sidebar's foot asks for the build; answered here,
// so the request never leaves the test.
vi.mock('./about', async (importOriginal) => {
  const { ABOUT } = await import('./testing/about')
  return {
    ...(await importOriginal<typeof import('./about')>()),
    about: vi.fn().mockResolvedValue(ABOUT),
  }
})

const TABLE = '/config/libraries'
/** The page that makes a library. */
const NEW_LIBRARY = `${TABLE}/new`
/** Manga's page, where its own controls and its folders' table are. */
const MANGA_PAGE = `${TABLE}/1`
/** The page of Light novels' only folder, where its Move… is. */
const NOVELS_FOLDER_PAGE = `${TABLE}/2/folders/3`

const folder = (
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
  folders: [
    folder(1, '/mnt/user/media 1/Manga/Volumes'),
    folder(2, '/mnt/user/media 1/Manga/Chapters (weekly)', {
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
  folders: [folder(3, '/mnt/user/media 1/Light Novels')],
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
    folders: [folder(10 + i, `/srv/shelf ${i + 1}`)],
  }),
)
/** Manga with folders enough that its page is taller than the window too. */
const LONG_MANGA: librariesApi.Library = {
  ...MANGA,
  folders: [
    ...MANGA.folders,
    ...Array.from({ length: 24 }, (_, i) =>
      folder(40 + i, `/mnt/user/media 1/Manga/Extras ${i + 1}`),
    ),
  ],
}
/** The list answers Light novels and the long Manga. */
const listLongManga = () =>
  vi.mocked(librariesApi.listLibraries).mockResolvedValue({
    libraries: [NOVELS, LONG_MANGA],
  })

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
      folders: [],
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

/** Whether `el` lies wholly in the window and clear of the phone bar. */
const expectInSight = (el: Element) => {
  const at = el.getBoundingClientRect()
  expect(at.top).toBeGreaterThanOrEqual(barBottom())
  expect(at.bottom).toBeLessThanOrEqual(window.innerHeight)
}

const STORE_LOCKED = new Error(
  '/api/library/libraries: 500 {"error":"the store is locked"}',
)

/** Renames the Manga library with a real mouse and keys, and lets it settle. */
async function renameManga(host: HTMLElement, name = 'Manga 2') {
  await click(host.querySelector('button[name="rename-library"]') as Element)
  const form = host.querySelector('form.rename-library') as HTMLElement
  await fill(form.querySelector('input') as Element, name)
  await click(form.querySelector('button[type="submit"]') as Element)
  await settle(200)
}

/** Renames Manga by script, which scrolls nothing. */
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

/** Opens the picker under Manga's folders with a real mouse. */
async function openAddFolder(host: HTMLElement) {
  await click(host.querySelector('button[name="add-folder"]') as Element)
  return host.querySelector('.library-page > .add-root') as HTMLElement
}

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

    it(`draws a refused Add folder where the person is looking, at ${width}`, async () => {
      vi.mocked(librariesApi.addFolder).mockRejectedValue(
        new Error(
          '/api/library/libraries/1/folders: 400 {"error":"/mnt/user/media 1/Manga/Volumes/Extras is inside /mnt/user/media 1/Manga/Volumes, which is already a folder of the library `Manga`"}',
        ),
      )
      const host = await shell(width, height, MANGA_PAGE)
      const picker = await openAddFolder(host)
      const box = picker.querySelector('input[name="path"]') as HTMLInputElement
      await click(box)
      await fill(box, '/mnt/user/media 1/Manga/Volumes/Extras')
      await click(picker.querySelector('button[name="pick"]') as Element)
      await settle()
      const alert = picker.querySelector('[role="alert"]') as HTMLElement
      expect(alert.textContent).toContain(
        ', which is already a folder of the library',
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
    // The form's rows are one gap apart, and so is the refusal from the
    // button it answers.
    expect(
      alert.getBoundingClientRect().top - submit.getBoundingClientRect().bottom,
    ).toBe(Number.parseFloat(getComputedStyle(form).rowGap))
  })

  for (const [width, height] of SIZES) {
    /**
     * O209: the pressed control at the very foot of the window, and its
     * refusal drawn beneath it, below the fold, is brought into view. Each
     * of these pages fits the window whole, and a control that is always in
     * view cannot be put at its foot, so the window is first made shorter
     * than the page down to the control.
     */
    async function atTheFoot(el: HTMLElement) {
      const main = document.querySelector('main') as HTMLElement
      const below =
        el.getBoundingClientRect().bottom + window.scrollY + main.scrollTop
      await page.viewport(width, Math.floor(below) - 100)
      await settle()
      el.scrollIntoView({ block: 'end' })
      await new Promise((r) => setTimeout(r, 60))
      expect(
        Math.abs(el.getBoundingClientRect().bottom - window.innerHeight),
      ).toBeLessThan(2)
    }

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
      await atTheFoot(make)
      await press(make)
      await settle()
      expectInSight(form.querySelector('[role="alert"]') as Element)
      expectInSight(make)
    })

    it(`brings a refused Add folder into view from the foot, at ${width}`, async () => {
      vi.mocked(librariesApi.addFolder).mockRejectedValue(
        new Error('/api/library/libraries/1/folders: 400 {"error":"refused"}'),
      )
      const host = await shell(width, height, MANGA_PAGE)
      const picker = await openAddFolder(host)
      await fill(
        picker.querySelector('input[name="path"]') as Element,
        '/srv/extra',
      )
      const pick = picker.querySelector('button[name="pick"]') as HTMLElement
      await atTheFoot(pick)
      await press(pick)
      await settle()
      expectInSight(picker.querySelector('[role="alert"]') as Element)
    })

    it(`brings a refused rename into view from the foot, at ${width}`, async () => {
      vi.mocked(librariesApi.renameLibrary).mockRejectedValue(
        new Error('/api/library/libraries/1: 400 {"error":"refused"}'),
      )
      const host = await shell(width, height, MANGA_PAGE)
      await click(
        host.querySelector('button[name="rename-library"]') as Element,
      )
      const form = host.querySelector('form.rename-library') as HTMLElement
      await fill(form.querySelector('input') as Element, 'Manga 2')
      const save = form.querySelector('button[type="submit"]') as HTMLElement
      await atTheFoot(save)
      await press(save)
      await settle()
      expectInSight(form.querySelector('[role="alert"]') as Element)
    })

    it(`draws a rename’s refusal on a line of its own beneath the form’s controls, at ${width}`, async () => {
      // A name short enough that the sentence would fit beside the buttons
      // at either size, were it let.
      vi.mocked(librariesApi.renameLibrary).mockRejectedValue(
        new Error(
          '/api/library/libraries/1: 400 {"error":"a library is already called `LN`"}',
        ),
      )
      const host = await shell(width, height, MANGA_PAGE)
      await renameManga(host, 'LN')
      const form = host.querySelector('form.rename-library') as HTMLElement
      const alert = form.querySelector('[role="alert"]') as HTMLElement
      expect(alert.textContent).toContain('is already called')
      const controls = [
        ...form.querySelectorAll('label.field, button'),
      ] as HTMLElement[]
      const lowest = Math.max(
        ...controls.map((c) => c.getBoundingClientRect().bottom),
      )
      const at = alert.getBoundingClientRect()
      // Beneath every control, one gap from the lowest, and nothing above it
      // beside the buttons.
      expect(at.top - lowest).toBe(
        Number.parseFloat(getComputedStyle(form).rowGap),
      )
      expect(at.left).toBe(form.getBoundingClientRect().left)
    })

    it(`brings the list’s failure into view after a rename whose reload fails, at ${width}`, async () => {
      vi.mocked(librariesApi.renameLibrary).mockResolvedValue({
        id: 1,
        name: 'Manga 2',
      } as never)
      listLongManga()
      const host = await shell(width, height, MANGA_PAGE)
      vi.mocked(librariesApi.listLibraries).mockRejectedValue(STORE_LOCKED)
      // At the foot of a long page, the failure's place, directly above the
      // page's buttons, is out of view. Renamed by script: a real press on
      // Rename… scrolls the buttons, and that place with them, into view.
      await toTheFoot(host)
      expect(
        (
          host.querySelector('.library-page > .actions') as Element
        ).getBoundingClientRect().top,
      ).toBeLessThan(barBottom())
      await renameMangaQuietly(host)
      const alert = listFailure(host, 'library-page')
      expect(alert?.textContent).toContain('the store is locked')
      expectInSight(alert as Element)
    })

    /**
     * The make page fits the window, so the new library's page opens at its
     * head, with the failure's place in view: this holds where the failure
     * is drawn, and that it is seen. That a failure out of view is brought
     * into it is held by the rename's and the removal's tests, whose pages
     * can be left scrolled away from that place.
     */
    it(`draws the list’s failure in sight on the new library’s page after a library is made and the reload fails, at ${width}`, async () => {
      vi.mocked(librariesApi.createLibrary).mockResolvedValue({
        library: {
          id: 3,
          name: 'Comics',
          works: 0,
          books: 0,
          folders: [folder(9, '/srv/comics')],
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
      expect(host.querySelector('.library-page > h2')?.textContent).toBe(
        'Comics',
      )
      const alert = listFailure(host, 'library-page')
      expect(alert?.textContent).toContain('the store is locked')
      expectInSight(alert as Element)
    })
  }

  /**
   * O200: the sidebar lists the libraries `/api/library/status` answers,
   * which the shell reads when it mounts. Both reads here answer what the
   * store holds, and the store has the library only once the form has made
   * it, so the sidebar can learn of it only by asking again.
   */
  it('lists a library the form makes in the sidebar, with no reload', async () => {
    const MADE: librariesApi.Library = {
      id: 3,
      name: 'Manga',
      works: 0,
      books: 0,
      folders: [folder(9, '/srv/manga')],
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
    await click(form.querySelector('button[type="submit"]') as Element)
    await settle(200)

    const links = [
      ...host.querySelectorAll<HTMLAnchorElement>('.sidebar-libraries a'),
    ]
    expect(links.map((a) => a.textContent)).toEqual(['Manga'])
    expect(links[0]?.getAttribute('href')).toBe('/?library=3')
  })

  /** The same, for a rename: the library is listed by its new name. */
  it('lists a renamed library by its new name in the sidebar, with no reload', async () => {
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

    await renameManga(host)

    expect(host.querySelector('.library-page > h2')?.textContent).toBe(
      'Manga 2',
    )
    expect(sidebarNames(host)).toEqual(['Light novels', 'Manga 2'])
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
   * The same, for a move: moving a library's only folder to a new library
   * removes the one and makes the other, and the sidebar follows both.
   */
  it('follows a move that makes one library and removes another in the sidebar, with no reload', async () => {
    const moved = () => vi.mocked(librariesApi.moveFolder).mock.calls.length > 0
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
    vi.mocked(librariesApi.moveFolder).mockResolvedValue(answer(false))
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
    const host = await shell(1280, 800, NOVELS_FOLDER_PAGE)
    expect(sidebarNames(host)).toEqual(['Light novels', 'Manga'])

    const move = [...host.querySelectorAll('.root-row button')].find(
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

    // Back at the foot of the page, the sentence still standing and out of
    // view, the same refusal answers another rename.
    await toTheFoot(host)
    expect(alert().getBoundingClientRect().bottom).toBeLessThan(barBottom())
    await renameMangaQuietly(host)
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

    // Wholly in the window, and under the bar: not in view.
    await putAt(host, alert(), 20)
    expect(alert().getBoundingClientRect().top).toBeLessThan(barBottom())
    await renameMangaQuietly(host)
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
          folders: [],
        }),
      )
      await click(host.querySelector('button[name="scan-library"]') as Element)
      await settle()
      const picker = await openAddFolder(host)
      await toTheFoot(host)
      const box = picker.querySelector('input[name="path"]') as HTMLInputElement
      await click(box)
      await keys('/srv/com')
      const reveal = vi.spyOn(Element.prototype, 'scrollIntoView')
      try {
        vi.mocked(librariesApi.listLibraries).mockRejectedValue(STORE_LOCKED)
        scanning = false
        await settle(1400)
        expect(listFailure(host, 'library-page')?.textContent).toContain(
          'the store is locked',
        )
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
    const slow = deferred<librariesApi.FolderListing>()
    vi.mocked(librariesApi.listFolders).mockImplementation((path) =>
      path === undefined
        ? Promise.resolve({
            path: null,
            parent: null,
            folders: [{ name: '/mnt/user', path: '/mnt/user' }],
          })
        : slow.promise,
    )
    const host = await shell(1280, 800, MANGA_PAGE)
    const picker = await openAddFolder(host)
    const box = picker.querySelector('input[name="path"]') as HTMLInputElement
    await click(picker.querySelector('button[name="browse"]') as Element)
    await settle()
    await click(picker.querySelector('.folder-list button') as Element)
    await click(box)
    await keys('/mnt/user/med')
    await act(async () => {
      slow.resolve({
        path: '/mnt/user',
        parent: null,
        folders: [{ name: 'media 1', path: '/mnt/user/media 1' }],
      })
      await Promise.resolve()
      await Promise.resolve()
    })
    await settle()
    expect(document.activeElement).toBe(box)
    await keys('ia')
    expect(box.value).toBe('/mnt/user/media')
  })
})
