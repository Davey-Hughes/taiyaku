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
  removeLibrary: vi.fn(),
  renameLibrary: vi.fn(),
  startLibraryScan: vi.fn(),
}))
vi.mock('./users', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./users')>()),
  listUsers: vi.fn(),
}))

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
    roots: [{ id: 1, name: 'Volumes' }],
    scanned: true,
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

async function shell(width: number, height: number): Promise<HTMLElement> {
  await page.viewport(width, height)
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <MemoryRouter initialEntries={['/config/libraries']}>
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

const card = (host: HTMLElement, name: string) =>
  [...host.querySelectorAll('.library-card')].find(
    (c) => c.querySelector(':scope > h3')?.textContent === name,
  ) as HTMLElement

/** Removes the Manga library from the foot of the page. */
async function removeManga(host: HTMLElement) {
  vi.mocked(librariesApi.libraryRemovalCost).mockResolvedValue({
    removed: { works: 0, positions: 0, notes: 0 },
    derive: emptyDerive,
    dry_run: true,
  })
  vi.mocked(librariesApi.removeLibrary).mockResolvedValue({
    removed: { works: 0, positions: 0, notes: 0 },
    derive: { ...emptyDerive, lost_positions: 4 },
    dry_run: false,
  })
  await toTheFoot(host)
  await press(
    card(host, 'Manga').querySelector(
      'button[name="remove-library"]',
    ) as Element,
  )
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
async function renameManga(host: HTMLElement) {
  const manga = card(host, 'Manga')
  await click(manga.querySelector('button[name="rename-library"]') as Element)
  const form = manga.querySelector('form.rename-library') as HTMLElement
  await fill(form.querySelector('input') as Element, 'Manga 2')
  await click(form.querySelector('button[type="submit"]') as Element)
  await settle(200)
}

describe('the Libraries screen in the real Shell', () => {
  it('draws a removal’s report clear of the phone bar, its opening sentence under nothing', async () => {
    const host = await shell(390, 844)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [NOVELS],
    })
    await removeManga(host)
    const report = host.querySelector('.removal-report') as HTMLElement
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
    const host = await shell(1280, 800)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [NOVELS],
    })
    // At the top of a page that fits, the report is in view as it lands.
    vi.mocked(librariesApi.libraryRemovalCost).mockResolvedValue({
      removed: { works: 0, positions: 0, notes: 0 },
      derive: emptyDerive,
      dry_run: true,
    })
    vi.mocked(librariesApi.removeLibrary).mockResolvedValue({
      removed: { works: 0, positions: 0, notes: 0 },
      derive: emptyDerive,
      dry_run: false,
    })
    try {
      const main = host.querySelector('main') as HTMLElement
      main.scrollTo(0, 0)
      await press(
        card(host, 'Manga').querySelector(
          'button[name="remove-library"]',
        ) as Element,
      )
      await press(host.querySelector('.remove-library .confirm') as Element)
      await settle()
      const report = host.querySelector('.removal-report') as HTMLElement
      expect(report.getBoundingClientRect().top).toBeGreaterThanOrEqual(0)
      expect(reveal).not.toHaveBeenCalled()
    } finally {
      reveal.mockRestore()
    }
  })

  for (const [width, height] of SIZES) {
    it(`draws the failure of the list that follows a removal in view with the report, at ${width}`, async () => {
      const host = await shell(width, height)
      vi.mocked(librariesApi.listLibraries).mockRejectedValue(
        new Error(
          '/api/library/libraries: 500 {"error":"the store is locked"}',
        ),
      )
      await removeManga(host)
      const alert = host.querySelector('.libraries > [role="alert"]')
      expect(alert?.textContent).toContain('the store is locked')
      const at = (alert as Element).getBoundingClientRect()
      expect(at.top).toBeGreaterThanOrEqual(barBottom())
      expect(at.bottom).toBeLessThanOrEqual(window.innerHeight)
      const head = (
        host.querySelector('.removal-report > p') as Element
      ).getBoundingClientRect()
      expect(head.top).toBeGreaterThanOrEqual(barBottom())
      expect(head.bottom).toBeLessThanOrEqual(window.innerHeight)
    })

    it(`draws a refused Add folder where the person is looking, at ${width}`, async () => {
      vi.mocked(librariesApi.addFolder).mockRejectedValue(
        new Error(
          '/api/library/libraries/1/folders: 400 {"error":"/mnt/user/media 1/Manga/Volumes/Extras is inside an existing folder"}',
        ),
      )
      const host = await shell(width, height)
      const manga = card(host, 'Manga')
      const picker = manga.querySelector(':scope > .add-root') as HTMLElement
      const box = picker.querySelector('input[name="path"]') as HTMLInputElement
      await click(box)
      await fill(box, '/mnt/user/media 1/Manga/Volumes/Extras')
      await click(picker.querySelector('button[name="pick"]') as Element)
      await settle()
      const alert = picker.querySelector('[role="alert"]') as HTMLElement
      expect(alert.textContent).toContain('is inside an existing folder')
      const at = alert.getBoundingClientRect()
      expect(at.top).toBeGreaterThanOrEqual(barBottom())
      expect(at.bottom).toBeLessThanOrEqual(window.innerHeight)
    })
  }

  it('gives the make form’s refusal the form’s own rhythm', async () => {
    vi.mocked(librariesApi.createLibrary).mockRejectedValue(
      new Error('/api/library/libraries: 400 {"error":"refused"}'),
    )
    const host = await shell(1280, 800)
    const form = host.querySelector('form.add-library') as HTMLFormElement
    await fill(
      form.querySelector('input[name="library-name"]') as Element,
      'Comics',
    )
    await fill(
      form.querySelector('input[name="path"]') as Element,
      '/srv/comics',
    )
    await click(form.querySelector('button[name="pick"]') as Element)
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
    it(`draws a rename’s refusal on a line of its own beneath the form’s controls, at ${width}`, async () => {
      vi.mocked(librariesApi.renameLibrary).mockRejectedValue(
        new Error(
          '/api/library/libraries/1: 400 {"error":"a library is already called `Light novels`"}',
        ),
      )
      const host = await shell(width, height)
      await renameManga(host)
      const form = card(host, 'Manga').querySelector(
        'form.rename-library',
      ) as HTMLElement
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
      const host = await shell(width, height)
      vi.mocked(librariesApi.listLibraries).mockRejectedValue(STORE_LOCKED)
      await renameManga(host)
      const alert = host.querySelector('.libraries > [role="alert"]')
      expect(alert?.textContent).toContain('the store is locked')
      expectInSight(alert as Element)
    })

    it(`brings the list’s failure into view after a library is made and the reload fails, at ${width}`, async () => {
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
      const host = await shell(width, height)
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
      await click(form.querySelector('button[name="pick"]') as Element)
      await click(form.querySelector('button[type="submit"]') as Element)
      await settle(200)
      const alert = host.querySelector('.libraries > [role="alert"]')
      expect(alert?.textContent).toContain('the store is locked')
      expectInSight(alert as Element)
    })
  }

  it('brings a failure into view again when the same sentence answers a second reload', async () => {
    vi.mocked(librariesApi.renameLibrary).mockResolvedValue({
      id: 1,
      name: 'Manga 2',
    } as never)
    const host = await shell(390, 844)
    vi.mocked(librariesApi.listLibraries).mockRejectedValue(STORE_LOCKED)
    await renameManga(host)
    expectInSight(host.querySelector('.libraries > [role="alert"]') as Element)

    // Back at the foot of the page, the sentence still standing and out of
    // view, the same refusal answers another rename.
    await toTheFoot(host)
    await renameManga(host)
    expectInSight(host.querySelector('.libraries > [role="alert"]') as Element)
  })

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

  /** Renames Manga by script, which scrolls nothing. */
  async function renameMangaQuietly(host: HTMLElement) {
    const manga = card(host, 'Manga')
    await press(manga.querySelector('button[name="rename-library"]') as Element)
    await typeInto(
      manga.querySelector('form.rename-library input') as HTMLInputElement,
      'Manga 3',
    )
    await submit(manga.querySelector('form.rename-library') as Element)
    await settle(200)
  }

  it('brings a failure that sits beneath the phone bar out from under it', async () => {
    vi.mocked(librariesApi.renameLibrary).mockResolvedValue({
      id: 1,
      name: 'Manga 2',
    } as never)
    const host = await shell(390, 844)
    vi.mocked(librariesApi.listLibraries).mockRejectedValue(STORE_LOCKED)
    await renameManga(host)
    const alert = () =>
      host.querySelector('.libraries > [role="alert"]') as HTMLElement
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
      const host = await shell(width, height)
      vi.mocked(librariesApi.listLibraries).mockRejectedValue(STORE_LOCKED)
      await renameMangaQuietly(host)
      const alert = () =>
        host.querySelector('.libraries > [role="alert"]') as HTMLElement
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
      expect(
        host.querySelector('.libraries > [role="alert"]')?.textContent,
      ).toContain('the store is locked')
      expect(reveal).not.toHaveBeenCalled()
    } finally {
      reveal.mockRestore()
    }
  })

  for (const [width, height] of SIZES) {
    it(`does not scroll a person typing at the foot when a scan ends and the list cannot be read, at ${width}`, async () => {
      let scanning = true
      vi.mocked(librariesApi.startLibraryScan).mockResolvedValue({
        library_id: 2,
        status_url: '/x',
      })
      const host = await shell(width, height)
      vi.mocked(librariesApi.libraryScanStatus).mockImplementation((id) =>
        Promise.resolve({
          library_id: id,
          running: id === 2 && scanning,
          elapsed_secs: 1,
          folders: [],
        }),
      )
      vi.mocked(config.scanStatus).mockImplementation((id) =>
        Promise.resolve({
          root_id: id,
          running: id === 3 && scanning,
          elapsed_secs: 1,
          report: null,
          last_scan_at: null,
          last_scan_status: null,
        }),
      )
      await click(
        card(host, 'Light novels').querySelector(
          'button[name="scan-library"]',
        ) as Element,
      )
      await settle()
      await toTheFoot(host)
      const nameBox = host.querySelector(
        'form.add-library input[name="library-name"]',
      ) as HTMLInputElement
      await click(nameBox)
      await keys('Com')
      const reveal = vi.spyOn(Element.prototype, 'scrollIntoView')
      try {
        vi.mocked(librariesApi.listLibraries).mockRejectedValue(STORE_LOCKED)
        scanning = false
        await settle(1400)
        expect(
          host.querySelector('.libraries > [role="alert"]')?.textContent,
        ).toContain('the store is locked')
        // The page was not told to scroll to the sentence, and the box the
        // person is typing in is still where they can see it.
        expect(reveal).not.toHaveBeenCalled()
        expectInSight(nameBox)
        expect(document.activeElement).toBe(nameBox)
        await keys('ics')
        expect(nameBox.value).toBe('Comics')
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
    const host = await shell(1280, 800)
    const picker = card(host, 'Manga').querySelector(
      ':scope > .add-root',
    ) as HTMLElement
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
