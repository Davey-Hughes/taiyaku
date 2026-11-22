import { act } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import * as aboutApi from '../src/about'
import type { Me } from '../src/api'
import { useBrowsePrefs } from '../src/browseModule'
import * as curation from '../src/curation'
import { GateContext } from '../src/gate'
import * as library from '../src/library'
import { Library } from '../src/Library'
import { LIBRARY_STATUS } from '../src/pageReads'
import { invalidate } from '../src/reads'
import { Series } from '../src/Series'
import { SIDEBAR_LAYOUT } from '../src/phoneLayout'
import { Shell } from '../src/Shell'
import {
  useInboxRefresh,
  useLibraryStatus,
  useStatusFailed,
} from '../src/shell'
import { ABOUT } from './support/about'
import { deferred, flush } from './support/async'
import { mount } from './support/mount'
import { fakeStorage } from './support/storage'

// `home`, `grid` and `oneSeries` for the tests that mount a page under the
// shell, so its View panel is the one a reader uses.
vi.mock('../src/library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/library')>()),
  status: vi.fn(),
  home: vi.fn(),
  grid: vi.fn(),
  oneSeries: vi.fn(),
}))

// Only the one call. `INBOX_MIN_SCORE` and `inboxTotal` are pure and are what
// the assertions below read the request and the badge through, so mocking
// them would leave the test agreeing with itself about a threshold the app
// does not use.
vi.mock('../src/curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/curation')>()),
  listProposals: vi.fn(),
}))

// The build's label at the sidebar's foot asks for the build;
// `VersionLabel.test.tsx` owns what it draws.
vi.mock('../src/about', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/about')>()),
  about: vi.fn(),
}))

const who = (over: Partial<Me> = {}): Me => ({
  user_id: 1,
  admin: false,
  can_download: false,
  can_stream: false,
  can_stamp: false,
  ...over,
})

/** Mount and let the status load settle — two flushes, as `Library.test.tsx` explains. */
async function settle(ui: React.ReactNode, me?: Me): Promise<HTMLElement> {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <MemoryRouter>
        <GateContext.Provider
          value={me ? { who: me, signOut: () => {} } : undefined}
        >
          {ui}
        </GateContext.Provider>
      </MemoryRouter>,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

/**
 * A child that reports what the context handed it.
 *
 * Declared inside a function rather than at module scope, which is where two
 * of this project's lint rules meet: `style/useComponentExportOnlyModules`
 * refuses an unexported component at module scope, and
 * `suspicious/noExportsInTest` refuses the export that would satisfy it — the
 * same trap `Gate.test.tsx`'s `probe()` documents.
 */
function reporter() {
  function Reporter() {
    const status = useLibraryStatus()
    return (
      <p data-testid="libraries">
        {status ? status.libraries.length : 'pending'}
      </p>
    )
  }
  return <Reporter />
}

/**
 * A child that reports `continueStrip` from `BrowsePrefsContext`, as a reader
 * that is neither the View panel nor the page, so the assertion reads the
 * provider's value directly.
 */
function continueStripProbe() {
  function Probe() {
    const prefs = useBrowsePrefs()
    return <p data-testid="continue-strip">{String(prefs.continueStrip)}</p>
  }
  return <Probe />
}

/**
 * A child that asks the chrome to re-count, on demand.
 *
 * Declared inside a function for `reporter()`'s reason — the two lint rules
 * its own comment names. This stands in for `Proposals.tsx`, which calls the
 * same hook from its `reload`; what belongs here is that the *shell* answers
 * it, and `Proposals.test.tsx` owns the half where a dismissal makes the call.
 */
function recountProbe() {
  function Probe() {
    const recount = useInboxRefresh()
    return (
      <button type="button" data-testid="recount" onClick={recount}>
        recount
      </button>
    )
  }
  return <Probe />
}

/**
 * A child that invalidates the status, on demand.
 *
 * Declared inside a function for `reporter()`'s reason. This stands in for
 * `Libraries.tsx`, which invalidates it after every write;
 * `Libraries.shell.browser.test.tsx` owns the half where a made or removed
 * library does.
 */
function rereadProbe() {
  function Probe() {
    return (
      <button
        type="button"
        data-testid="reread"
        onClick={() => {
          void invalidate(LIBRARY_STATUS)
        }}
      >
        reread
      </button>
    )
  }
  return <Probe />
}

/** The one series the library page lists, and the series page draws. */
const ONE_SERIES: library.SeriesCard = {
  id: 7,
  title: 'A Series',
  works: 3,
  editions: 3,
  aligned: 1,
  added: 10,
  unread: 3,
  cover: '/api/works/a-series-v01/raw/pages/0/thumb?v=c0ffee-0-1',
}

/** The names the sidebar's libraries list draws, in its order. */
const sidebarLibraries = (host: HTMLElement) =>
  [...host.querySelectorAll('.sidebar-libraries a')].map((a) => a.textContent)

/**
 * A child that reports what a screen under the shell is told of the status:
 * whether its read failed with none kept, and how many libraries it names.
 * Declared inside a function for the reason `reporter()` gives.
 */
function failedProbe() {
  function Probe() {
    const failed = useStatusFailed()
    const status = useLibraryStatus()
    return (
      <p data-testid="status-failed">
        {JSON.stringify({
          failed,
          libraries: status?.libraries.length ?? null,
        })}
      </p>
    )
  }
  return <Probe />
}

/** Presses `rereadProbe()`'s button and lets the read it sets off land. */
async function reread(host: HTMLElement) {
  const button = host.querySelector<HTMLElement>('[data-testid="reread"]')
  if (!button) throw new Error('no reread control rendered')
  await act(async () => {
    button.click()
    await Promise.resolve()
    await Promise.resolve()
  })
}

/** The page's View panel, opened, failing if it did not open. */
function openView(host: HTMLElement): HTMLElement {
  const button = host.querySelector<HTMLButtonElement>(
    'main button.view-button',
  )
  const id = button?.getAttribute('aria-controls')
  const panel = id ? document.getElementById(id) : null
  if (!button || !panel) throw new Error('the page draws no View panel')
  if (panel.hidden) {
    act(() => {
      button.click()
    })
  }
  expect(button.getAttribute('aria-expanded')).toBe('true')
  expect(panel.hidden).toBe(false)
  return panel
}

/**
 * The control the View panel's `<label>` whose own text is `label` names:
 * a generated row's words sit in a `<span>` beside its control, and the
 * Cover size slider's label holds bare text and names its range by `for`.
 */
function controlLabelled(panel: HTMLElement, label: string): HTMLElement {
  const row = [...panel.querySelectorAll('label')].find(
    (l) => (l.querySelector('span') ?? l).textContent === label,
  )
  const control = row?.control
  if (!control) throw new Error(`no control labelled "${label}"`)
  return control
}

/** The checkbox in the open View panel labelled `label`. */
function checkboxLabelled(host: HTMLElement, label: string): HTMLInputElement {
  const box = controlLabelled(openView(host), label)
  if (!(box instanceof HTMLInputElement) || box.type !== 'checkbox') {
    throw new Error(`no checkbox labelled "${label}"`)
  }
  return box
}

/** The View panel's Reset. */
function resetIn(host: HTMLElement): HTMLButtonElement {
  const reset = [...openView(host).querySelectorAll('button')].find(
    (b) => b.textContent === 'Reset',
  )
  if (!reset) throw new Error('the View panel has no Reset button')
  return reset
}

/** Follows a link with `click()`: no pointer press and no focus move. */
async function follow(host: HTMLElement, selector: string) {
  const link = host.querySelector<HTMLAnchorElement>(selector)
  if (!link) throw new Error(`no ${selector}`)
  await act(async () => {
    link.click()
    await Promise.resolve()
    await Promise.resolve()
  })
}

/** ☰, found by the column it controls rather than by its icon. */
function menuButton(host: HTMLElement): HTMLButtonElement {
  const button = host.querySelector<HTMLButtonElement>(
    'button[aria-controls="library-menu"]',
  )
  if (!button) throw new Error('no menu button')
  return button
}

/** The sidebar column, by the id ☰ names. */
function menu(host: HTMLElement): HTMLElement {
  const column = host.querySelector<HTMLElement>('#library-menu')
  if (!column) throw new Error('nothing carries the id ☰ controls')
  return column
}

function openMenu(host: HTMLElement) {
  act(() => {
    menuButton(host).click()
  })
}

/**
 * A `matchMedia` for jsdom, which has none: one list whose `matches` the test
 * flips, calling its listeners the way a window resized out of the phone
 * layout would.
 */
function fakeMatchMedia() {
  const listeners = new Set<() => void>()
  const list = {
    matches: false,
    addEventListener: (_type: string, listener: () => void) => {
      listeners.add(listener)
    },
    removeEventListener: (_type: string, listener: () => void) => {
      listeners.delete(listener)
    },
  }
  const matchMedia = vi.fn((_query: string) => list)
  const widen = () => {
    list.matches = true
    for (const listener of listeners) listener()
  }
  return { matchMedia, widen }
}

describe('Shell', () => {
  beforeEach(() => {
    // Call records, not implementations — `asks for no count at all for a
    // reader` asserts on *not* having been called, and without this it reads
    // the admin mounts above it.
    vi.clearAllMocks()
    vi.mocked(aboutApi.about).mockResolvedValue(ABOUT)
    vi.mocked(library.status).mockResolvedValue({
      libraries: [{ id: 1, name: 'Manga', scanned: true }],
      restricted: false,
    })
    vi.mocked(curation.listProposals).mockResolvedValue({
      proposals: [],
      counts: { 'edition-of': 5, 'same-series': 2 },
    })
    vi.mocked(library.home).mockResolvedValue({
      continuing: [],
      added: [],
      read: [],
    })
    vi.mocked(library.grid).mockResolvedValue({
      series: [ONE_SERIES],
      unfiled: [],
    })
    vi.mocked(library.oneSeries).mockResolvedValue({
      series: {
        ...ONE_SERIES,
        titles: [{ title: ONE_SERIES.title, kind: 'romaji', primary: true }],
      },
      works: [],
    })
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('fetches the status once and offers it to what it renders', async () => {
    const host = await settle(<Shell>{reporter()}</Shell>)
    expect(host.querySelector('[data-testid="libraries"]')?.textContent).toBe(
      '1',
    )
    expect(vi.mocked(library.status)).toHaveBeenCalledTimes(1)
  })

  it('offers the build’s label, More, then the theme switch at the foot of the sidebar', async () => {
    vi.stubGlobal('localStorage', fakeStorage())
    const host = await settle(<Shell>{null}</Shell>)
    const column = host.querySelector('.sidebar-column')
    const foot = column?.lastElementChild
    expect(foot?.classList.contains('sidebar-foot')).toBe(true)
    expect([...(foot?.children ?? [])].map((el) => el.className)).toEqual([
      'version-label',
      'account-menu',
      'theme-control',
    ])
    // More is a navigation landmark of its own; Theme is a setting, in none.
    expect(foot?.querySelector('.theme-control')?.closest('nav')).toBeNull()
    expect(
      column?.querySelectorAll('.theme-control input[type="radio"]'),
    ).toHaveLength(3)
  })

  /**
   * Spec F §3: the libraries list is every account's, not an admin's
   * preference. The status is what the server lets this account see, so the
   * shell hands all of it on, to a reader as to an admin.
   */
  it('hands the sidebar every library the status lists, to any account', async () => {
    vi.mocked(library.status).mockResolvedValue({
      libraries: [
        { id: 1, name: 'Manga', scanned: true },
        { id: 2, name: 'Comics', scanned: false },
      ],
      restricted: false,
    })
    const host = await settle(<Shell>x</Shell>, who({ admin: false }))
    expect(
      [...host.querySelectorAll('.sidebar-libraries a')].map(
        (a) => a.textContent,
      ),
    ).toEqual(['Manga', 'Comics'])
  })

  /**
   * More's proposals link is admin-only, and `AccountMenu` only knows that
   * from its `admin` prop — `Sidebar.test.tsx` constructs `<AccountMenu>`
   * directly, so nothing there can pin that `Shell` threads the gate's
   * `admin` bit into it. Without this, a wrong value in that line would
   * leave every admin account seeing no link, silently.
   */
  it('gives More the account’s admin bit, so the proposals link follows it too', async () => {
    const asAdmin = await settle(
      <Shell>
        <p>x</p>
      </Shell>,
      who({ admin: true }),
    )
    expect(asAdmin.querySelector('a[href="/proposals"]')).not.toBeNull()

    const asReader = await settle(
      <Shell>
        <p>x</p>
      </Shell>,
      who({ admin: false }),
    )
    expect(asReader.querySelector('a[href="/proposals"]')).toBeNull()
  })

  /**
   * O94: the account screen is the one entry that follows no account's role,
   * and for anyone but an administrator More is the link to it (spec §1).
   * `Sidebar.test.tsx` pins it against the prop; this pins it through the
   * chrome a signed-in reader actually gets, with the gate's `admin` bit
   * false.
   */
  it('offers a signed-in reader the account screen, as More', async () => {
    const asReader = await settle(
      <Shell>
        <p>x</p>
      </Shell>,
      who({ admin: false }),
    )
    const more = asReader.querySelector('.sidebar-foot a[href="/account"]')
    expect(more?.textContent).toBe('More')
    expect(more?.hasAttribute('aria-expanded')).toBe(false)
  })

  /**
   * The count itself. `Sidebar.test.tsx` pins that a number handed to
   * `AccountMenu` is drawn; nothing there can pin that `Shell` fetches one, or
   * *which* one — deleting this effect leaves every sidebar test green and
   * every deployment with a bare `Proposals` entry.
   *
   * The two kind counts differ (5 and 2), so `7` cannot come from either one
   * alone, and the request's own filter is asserted: a count taken at no
   * threshold, or across every state, is a badge that disagrees with the
   * screen it links to.
   */
  it('reads the open count for an admin, at the threshold the inbox opens at', async () => {
    const host = await settle(
      <Shell>
        <p>x</p>
      </Shell>,
      who({ admin: true }),
    )
    expect(vi.mocked(curation.listProposals)).toHaveBeenCalledWith({
      state: 'open',
      min_score: curation.INBOX_MIN_SCORE,
    })
    expect(host.querySelector('a[href="/proposals"] .count')?.textContent).toBe(
      '7',
    )
    expect(host.querySelector('.account-button .count')?.textContent).toBe('7')
  })

  it('re-reads the count when a screen under it says a write moved the inbox', async () => {
    // `Shell` fetches once on mount; the inbox screen re-reads its own list
    // after every accept and dismiss, and without this seam the badge sits at
    // the mount's number beside a header that has moved. The second answer
    // differs from the first so "it asked again" and "it drew the first
    // answer twice" are distinguishable.
    vi.mocked(curation.listProposals)
      .mockResolvedValueOnce({
        proposals: [],
        counts: { 'edition-of': 5, 'same-series': 2 },
      })
      .mockResolvedValue({ proposals: [], counts: { 'edition-of': 4 } })

    const host = await settle(
      <Shell>{recountProbe()}</Shell>,
      who({ admin: true }),
    )
    expect(host.querySelector('a[href="/proposals"] .count')?.textContent).toBe(
      '7',
    )

    const button = host.querySelector<HTMLElement>('[data-testid="recount"]')
    if (!button) throw new Error('no recount control rendered')
    await act(async () => {
      button.click()
      await Promise.resolve()
      await Promise.resolve()
    })

    expect(vi.mocked(curation.listProposals)).toHaveBeenCalledTimes(2)
    expect(host.querySelector('a[href="/proposals"] .count')?.textContent).toBe(
      '4',
    )
  })

  it('asks for no count at all for a reader, who has no inbox to count', async () => {
    await settle(
      <Shell>
        <p>x</p>
      </Shell>,
      who({ admin: false }),
    )
    expect(vi.mocked(curation.listProposals)).not.toHaveBeenCalled()
  })

  /**
   * The badge is an ornament and its failure is not the reader's problem —
   * but "swallowed" must mean *the link still works*, not "the shell throws
   * an alert over every screen in the library". The status fetch beside it
   * does report, and the two are deliberately different; this is the test
   * that says which is which.
   */
  it('keeps the proposals link and stays quiet when the count cannot be read', async () => {
    vi.mocked(curation.listProposals).mockRejectedValue(
      new Error('/api/library/proposals: 500'),
    )
    const host = await settle(
      <Shell>
        <p>x</p>
      </Shell>,
      who({ admin: true }),
    )
    expect(host.querySelector('a[href="/proposals"]')).not.toBeNull()
    expect(host.querySelector('a[href="/proposals"] .count')).toBeNull()
    expect(host.querySelector('[role="alert"]')).toBeNull()
  })

  /**
   * Pins `gate.ts`'s fail-safe default at the one place it can actually be
   * seen: `useGate()`'s doc comment says "Getting `undefined` therefore has
   * to be safe, and the safe reading is 'no account known', never 'an
   * admin'." `Shell.tsx` reads that as `useGate()?.who.admin ?? false`, and
   * this is the only test in this file that mounts with no gate above it
   * *and* looks at the admin-only entries: the proposals tests above always
   * mount with a provider present, and none of this file's other no-provider
   * tests looks at an admin entry.
   *
   * The reporter child proves the status landed, and the sidebar draws its
   * library, before the admin assertions run — so an absent entry below can
   * only be the `?? false` default, not a sidebar that never drew. More is
   * then the plain link to the account screen, which every account gets.
   */
  it('draws no admin entries where no gate sits above it', async () => {
    const host = await settle(<Shell>{reporter()}</Shell>)
    expect(host.querySelector('[data-testid="libraries"]')?.textContent).toBe(
      '1',
    )
    expect(host.querySelector('.sidebar-libraries a')?.textContent).toBe(
      'Manga',
    )
    expect(host.querySelector('a[href="/proposals"]')).toBeNull()
    expect(host.querySelector('a[href="/config"]')).toBeNull()
    expect(host.querySelector('button.account-button')).toBeNull()
    expect(
      host.querySelector('.sidebar-foot a[href="/account"]')?.textContent,
    ).toBe('More')
  })

  it('says so when the status cannot be read, rather than drawing an empty sidebar in silence', async () => {
    vi.mocked(library.status).mockRejectedValue(
      new Error('/api/library/status: 500'),
    )
    const host = await settle(
      <Shell>
        <p>x</p>
      </Shell>,
    )
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('500')
  })

  /**
   * The shell says the status's failure through `message.ts`'s `messageOf`,
   * and every other test in this file that fails the read rejects with a
   * real `Error`, so its `String(err)` half runs here and nowhere else on
   * this screen. A `fetch` rejection is not always an `Error`: a request
   * `AbortController` cancels, or a mock in another test, can reject with
   * anything at all, and this is what the reader sees when it does.
   */
  it('stringifies a rejection that is not an Error, rather than losing it', async () => {
    vi.mocked(library.status).mockRejectedValue('offline')
    const host = await settle(
      <Shell>
        <p>x</p>
      </Shell>,
    )
    expect(host.querySelector('[role="alert"]')?.textContent).toBe('offline')
  })

  /**
   * O200: `Shell` reads the status once on mount, and Configuration →
   * Libraries makes and removes libraries under the sidebar that lists them.
   * The second answer differs from the first, so "it asked again" and "it
   * drew the first answer twice" are told apart.
   */
  it('re-reads the status when a write under it invalidates it', async () => {
    vi.mocked(library.status)
      .mockResolvedValueOnce({
        libraries: [{ id: 1, name: 'Manga', scanned: true }],
        restricted: false,
      })
      .mockResolvedValue({
        libraries: [{ id: 2, name: 'Comics', scanned: false }],
        restricted: false,
      })
    const host = await settle(<Shell>{rereadProbe()}</Shell>)
    expect(sidebarLibraries(host)).toEqual(['Manga'])

    await reread(host)

    expect(vi.mocked(library.status)).toHaveBeenCalledTimes(2)
    expect(sidebarLibraries(host)).toEqual(['Comics'])
  })

  /**
   * Reads need not answer in order. A re-read asked for while the first read
   * is still out answers for the store as it is since the write, so the first
   * read's answer, landing after it, is the older one and is dropped.
   */
  it('keeps a re-read when the read before it answers after it', async () => {
    const first = deferred<library.LibraryStatus>()
    const second = deferred<library.LibraryStatus>()
    vi.mocked(library.status)
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise)
    const host = await settle(<Shell>{rereadProbe()}</Shell>)
    await reread(host)
    expect(vi.mocked(library.status)).toHaveBeenCalledTimes(2)

    await flush(() =>
      second.resolve({
        libraries: [{ id: 2, name: 'Comics', scanned: true }],
        restricted: false,
      }),
    )
    expect(sidebarLibraries(host)).toEqual(['Comics'])
    await flush(() =>
      first.resolve({
        libraries: [{ id: 1, name: 'Manga', scanned: true }],
        restricted: false,
      }),
    )
    expect(sidebarLibraries(host)).toEqual(['Comics'])
  })

  /** The same, for an older read that fails after the re-read has landed. */
  it('keeps a re-read when the read before it fails after it', async () => {
    const first = deferred<library.LibraryStatus>()
    const second = deferred<library.LibraryStatus>()
    vi.mocked(library.status)
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise)
    const host = await settle(<Shell>{rereadProbe()}</Shell>)
    await reread(host)
    expect(vi.mocked(library.status)).toHaveBeenCalledTimes(2)

    await flush(() =>
      second.resolve({
        libraries: [{ id: 2, name: 'Comics', scanned: true }],
        restricted: false,
      }),
    )
    await flush(() => first.reject(new Error('/api/library/status: 500')))
    expect(host.querySelector('[role="alert"]')).toBeNull()
    expect(sidebarLibraries(host)).toEqual(['Comics'])
  })

  /**
   * A re-read that fails says so as the first read's failure does. The
   * libraries already drawn stay: the failure is drawn beside them, and the
   * links still lead somewhere the server will answer for.
   */
  it('says so when a re-read fails, and keeps the libraries it last read', async () => {
    vi.mocked(library.status)
      .mockResolvedValueOnce({
        libraries: [{ id: 1, name: 'Manga', scanned: true }],
        restricted: false,
      })
      .mockRejectedValue(new Error('/api/library/status: 500'))
    const host = await settle(<Shell>{rereadProbe()}</Shell>)
    expect(host.querySelector('[role="alert"]')).toBeNull()

    await reread(host)

    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      '/api/library/status: 500',
    )
    expect(sidebarLibraries(host)).toEqual(['Manga'])
  })

  /**
   * A screen waiting on the status (the Library view, before it offers its
   * search box) must hear that the read failed, since nothing else will
   * answer it; once a status is kept, a failed re-read leaves it drawn.
   */
  it('tells the screens under it that the status read failed only while no status is kept', async () => {
    const first = deferred<library.LibraryStatus>()
    vi.mocked(library.status)
      .mockReturnValueOnce(first.promise)
      .mockResolvedValueOnce({
        libraries: [{ id: 1, name: 'Manga', scanned: true }],
        restricted: false,
      })
      .mockRejectedValueOnce(new Error('/api/library/status: 500'))
    const told = (host: HTMLElement) =>
      JSON.parse(
        host.querySelector('[data-testid="status-failed"]')?.textContent ??
          'null',
      )
    const host = await settle(
      <Shell>
        {failedProbe()}
        {rereadProbe()}
      </Shell>,
    )
    // In flight is not failed: a screen waiting on the status keeps waiting.
    expect(told(host)).toEqual({ failed: false, libraries: null })
    await flush(() => first.reject(new Error('/api/library/status: 500')))
    await flush()
    expect(told(host)).toEqual({ failed: true, libraries: null })
    await reread(host)
    expect(told(host)).toEqual({ failed: false, libraries: 1 })
    await reread(host)
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      '/api/library/status: 500',
    )
    expect(told(host)).toEqual({ failed: false, libraries: 1 })
  })

  /**
   * Spec O285 item 10: a stored `density` carries over to a size once, a
   * stored `coverSize` wins over it, and one off the scale is drawn at its
   * nearest position but saved as it was. Seeded through storage rather
   * than through a View panel's control, because this is about what `Shell`
   * draws from a stored preference on first mount and what its first save
   * leaves behind, not about the panel that writes one.
   */
  it.each([
    ['a stored compact density', { density: 'compact' }, '104', 104],
    ['a stored comfortable density', { density: 'comfortable' }, '144', 144],
    ['nothing stored', undefined, '144', 144],
    ['a stored size', { coverSize: 200 }, '200', 200],
    [
      'a stored size beside a density',
      { coverSize: 200, density: 'compact' },
      '200',
      200,
    ],
    ['a stored size off the scale', { coverSize: 151 }, '152', 151],
  ] as const)(
    'draws the covers from %s, and saves their size with no density',
    async (_, stored, drawn, saved) => {
      const s = fakeStorage()
      if (stored) s.setItem('taiyaku.prefs.browse', JSON.stringify(stored))
      vi.stubGlobal('localStorage', s)
      const host = await settle(
        <Shell>
          <p>x</p>
        </Shell>,
      )
      const root = host.querySelector<HTMLElement>('.library')
      expect(root?.style.getPropertyValue('--cover-n')).toBe(drawn)
      const slice = JSON.parse(s.getItem('taiyaku.prefs.browse') ?? 'null')
      expect(slice).toMatchObject({ coverSize: saved })
      expect(slice).not.toHaveProperty('density')
    },
  )

  /**
   * `module.ts`'s `sliceReset` doc: `resetPrefs` alone satisfies "reload,
   * confirm it persisted" and silently fails "change a setting, confirm it
   * takes effect" — the in-memory half alone is the mirror failure, a reload
   * resurrecting the old value. Neither half proves the other, so this pins
   * the one `ViewMenu.test.tsx` cannot: `resetPrefs(browseModule.id)`
   * actually clearing `localStorage`, not just `reset()`'s in-memory
   * `sliceReset` making the panel agree with itself. The strip is the
   * library page's, so it is that page's View that changes and resets it.
   *
   * A fresh `mount()` is what stands in for a reload — a second render of the
   * same instance would only re-read the state already in memory, which is
   * exactly the half this test must not credit.
   */
  it('reset clears storage, so a fresh mount does not resurrect the changed value', async () => {
    vi.stubGlobal('localStorage', fakeStorage())

    const before = await settle(
      <Shell>
        <Library />
      </Shell>,
    )
    const stripBefore = checkboxLabelled(before, 'Continue strip')
    expect(stripBefore.checked).toBe(true)
    act(() => {
      stripBefore.click()
    })
    expect(stripBefore.checked).toBe(false)

    const resetButton = resetIn(before)
    act(() => {
      resetButton.click()
    })
    before.remove()

    // A fresh mount, standing in for a reload: `loadPrefs` reads whatever
    // `resetPrefs` left in storage, not the map still sitting in the first
    // instance's memory.
    const after = await settle(
      <Shell>
        <Library />
      </Shell>,
    )
    expect(checkboxLabelled(after, 'Continue strip').checked).toBe(true)
  })

  /**
   * `BrowsePrefsContext.Provider` is the one wire from `Shell`'s own `prefs`
   * state to every browse screen mounted below it. This pins that the value
   * handed to `value={…}` is `prefs` itself rather than some constant that
   * merely has the right shape. `module.ts`'s `sliceReset` doc names the test
   * outright: "change a setting, confirm it takes effect" — written here at
   * the `Shell` level, through a probe that reads the context the way a real
   * screen (`Series`, `SeriesGrid`'s caller) would, beside the library page
   * whose View makes the change.
   */
  it('threads a changed preference from the library page’s View to whatever reads the context below it', async () => {
    const host = await settle(
      <Shell>
        <Library />
        {continueStripProbe()}
      </Shell>,
    )
    expect(
      host.querySelector('[data-testid="continue-strip"]')?.textContent,
    ).toBe('true')

    const strip = checkboxLabelled(host, 'Continue strip')
    expect(strip.checked).toBe(true)
    act(() => {
      strip.click()
    })

    expect(
      host.querySelector('[data-testid="continue-strip"]')?.textContent,
    ).toBe('false')
  })

  /**
   * The reset test above pins the mirror case — that a reload does *not*
   * resurrect a value nothing ever stored — which a deleted `persistPrefs`
   * call satisfies for free, since with nothing ever written a fresh mount
   * reads defaults either way. This is the direction that deletion breaks: a
   * preference the reader *changed* has to still be there after a reload. A
   * fresh `mount()` stands in for the reload, same as the reset test's
   * `before`/`after` pair.
   */
  it('a changed preference survives a reload, not just a reset', async () => {
    vi.stubGlobal('localStorage', fakeStorage())

    const before = await settle(
      <Shell>
        <Library />
      </Shell>,
    )
    const stripBefore = checkboxLabelled(before, 'Continue strip')
    expect(stripBefore.checked).toBe(true)
    act(() => {
      stripBefore.click()
    })
    expect(stripBefore.checked).toBe(false)
    before.remove()

    // A fresh mount, standing in for a reload: `loadPrefs` reads whatever
    // `usePrefsMap`'s persist effect actually wrote, not the map still
    // sitting in the first instance's memory.
    const after = await settle(
      <Shell>
        <Library />
      </Shell>,
    )
    expect(checkboxLabelled(after, 'Continue strip').checked).toBe(false)
  })

  /**
   * Spec §2: Reset restores every browse preference from either page. The
   * library page's View shows two of them, so a Reset that restored only
   * what its panel shows would leave the series page's four as they were.
   */
  it('restores the series page’s four as well on a Reset from the library page', async () => {
    const s = fakeStorage()
    s.setItem(
      'taiyaku.prefs.browse',
      JSON.stringify({
        layout: 'table',
        editionSelector: false,
        panel: 'off',
        coverSize: 104,
        continueStrip: false,
        alwaysShowAligned: true,
      }),
    )
    vi.stubGlobal('localStorage', s)
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <MemoryRouter initialEntries={['/series/7']}>
          <Shell>
            <Routes>
              <Route path="/" element={<Library />} />
              <Route path="/series/:id" element={<Series />} />
            </Routes>
          </Shell>
        </MemoryRouter>,
      )
      await Promise.resolve()
      await Promise.resolve()
    })
    if (!host) throw new Error('nothing mounted')
    const seriesView = (at: HTMLElement) => {
      const panel = openView(at)
      return {
        layout: (controlLabelled(panel, 'Layout') as HTMLSelectElement).value,
        editionSelector: (
          controlLabelled(panel, 'Edition selector') as HTMLInputElement
        ).checked,
        panel: (controlLabelled(panel, 'Panel') as HTMLSelectElement).value,
        alwaysShowAligned: (
          controlLabelled(panel, 'Always show aligned') as HTMLInputElement
        ).checked,
      }
    }
    expect(seriesView(host)).toEqual({
      layout: 'table',
      editionSelector: false,
      panel: 'off',
      alwaysShowAligned: true,
    })

    /** The Cover size slider's position and the size its output says. */
    const coverSize = (panel: HTMLElement) => {
      const range = controlLabelled(panel, 'Cover size') as HTMLInputElement
      const output = [...panel.querySelectorAll('output')].find(
        (o) => o.getAttribute('for') === range.id,
      )
      return [range.value, output?.textContent]
    }

    await follow(host, '.sidebar-heading a[href="/"]')
    expect(checkboxLabelled(host, 'Continue strip').checked).toBe(false)
    expect(coverSize(openView(host))).toEqual(['8', '104 px'])
    const reset = resetIn(host)
    act(() => {
      reset.click()
    })
    const libraryView = openView(host)
    expect(
      (controlLabelled(libraryView, 'Continue strip') as HTMLInputElement)
        .checked,
    ).toBe(true)
    expect(coverSize(libraryView)).toEqual(['48', '144 px'])

    await follow(host, 'main a[href="/series/7"]')
    expect(seriesView(host)).toEqual({
      layout: 'grid',
      editionSelector: true,
      panel: 'button',
      alwaysShowAligned: false,
    })
  })

  /**
   * The drawer — spec 2026-09-17 §7.1. jsdom lays nothing out, so this pins
   * the state and the attributes; `Shell.browser.test.tsx` measures what a
   * browser does with them: where Tab can go, whether the page scrolls
   * behind the drawer, where the column sits.
   */
  describe('the menu in the phone layout', () => {
    it('draws ☰ closed, naming the column it opens, with nothing inert', async () => {
      const host = await settle(
        <Shell>
          <p>x</p>
        </Shell>,
      )
      const button = menuButton(host)
      expect(button.getAttribute('aria-label')).toBe('Menu')
      expect(button.getAttribute('aria-expanded')).toBe('false')
      expect(menu(host).classList.contains('sidebar-column')).toBe(true)
      expect(menu(host).hasAttribute('role')).toBe(false)
      expect(menu(host).hasAttribute('aria-modal')).toBe(false)
      expect(host.querySelector('.phone-bar')?.hasAttribute('inert')).toBe(
        false,
      )
      expect(host.querySelector('main')?.hasAttribute('inert')).toBe(false)
      expect(host.querySelector('.scrim')).toBeNull()
    })

    it('opens the column as a modal dialog and makes what is behind it inert', async () => {
      const host = await settle(
        <Shell>
          <p>x</p>
        </Shell>,
      )
      openMenu(host)
      expect(menuButton(host).getAttribute('aria-expanded')).toBe('true')
      const column = menu(host)
      expect(column.getAttribute('role')).toBe('dialog')
      expect(column.getAttribute('aria-modal')).toBe('true')
      expect(column.getAttribute('aria-label')).toBe('Menu')
      expect(column.hasAttribute('inert')).toBe(false)
      expect(host.querySelector('.phone-bar')?.hasAttribute('inert')).toBe(true)
      expect(host.querySelector('main')?.hasAttribute('inert')).toBe(true)
      expect(
        host.querySelector('.library')?.classList.contains('menu-open'),
      ).toBe(true)
    })

    it('takes focus to the Libraries heading’s link, not the wordmark’s above it, and gives it back to ☰ on Escape', async () => {
      const host = await settle(
        <Shell>
          <p>x</p>
        </Shell>,
      )
      openMenu(host)
      const focused = document.activeElement
      expect([
        focused === menu(host).querySelector('.sidebar-heading a'),
        focused?.textContent,
        focused?.getAttribute('aria-label'),
      ]).toEqual([true, 'Libraries', null])
      act(() => {
        document.dispatchEvent(
          new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }),
        )
      })
      expect(menuButton(host).getAttribute('aria-expanded')).toBe('false')
      expect(document.activeElement).toBe(menuButton(host))
    })

    it('closes from the Close button inside it, and gives focus back to ☰', async () => {
      const host = await settle(
        <Shell>
          <p>x</p>
        </Shell>,
      )
      openMenu(host)
      const close =
        menu(host).querySelector<HTMLButtonElement>('button.menu-close')
      expect(close?.getAttribute('aria-label')).toBe('Close menu')
      act(() => {
        close?.click()
      })
      expect(menuButton(host).getAttribute('aria-expanded')).toBe('false')
      expect(document.activeElement).toBe(menuButton(host))
    })

    it('closes on a tap on the page behind it', async () => {
      const host = await settle(
        <Shell>
          <p>x</p>
        </Shell>,
      )
      openMenu(host)
      const scrim = host.querySelector<HTMLButtonElement>('.scrim')
      expect(scrim?.getAttribute('aria-label')).toBe('Close menu')
      act(() => {
        scrim?.click()
      })
      expect(menuButton(host).getAttribute('aria-expanded')).toBe('false')
      expect(host.querySelector('.scrim')).toBeNull()
    })

    it('closes when a link in it is followed', async () => {
      const host = await settle(
        <Shell>
          <p>x</p>
        </Shell>,
      )
      openMenu(host)
      const manga = menu(host).querySelector<HTMLAnchorElement>(
        'a[href="/?library=1"]',
      )
      if (!manga) throw new Error('no library link in the menu')
      await act(async () => {
        manga.click()
      })
      expect(menuButton(host).getAttribute('aria-expanded')).toBe('false')
    })

    it('closes when the window leaves the phone layout, where the column is the sidebar again', async () => {
      const media = fakeMatchMedia()
      vi.stubGlobal('matchMedia', media.matchMedia)
      const host = await settle(
        <Shell>
          <p>x</p>
        </Shell>,
      )
      openMenu(host)
      expect(media.matchMedia).toHaveBeenCalledWith(SIDEBAR_LAYOUT)
      act(() => {
        media.widen()
      })
      expect(menuButton(host).getAttribute('aria-expanded')).toBe('false')
    })

    it('draws the wordmark at the top of the column as a link to /, and as the bar’s way home, each named for both', async () => {
      const host = await settle(
        <Shell>
          <p>x</p>
        </Shell>,
      )
      const top = menu(host).firstElementChild
      expect([
        top?.tagName,
        top?.getAttribute('href'),
        top?.getAttribute('aria-label'),
        top?.firstElementChild?.classList.contains('wordmark'),
      ]).toEqual(['A', '/', 'taiyaku, Libraries', true])
      const home = host.querySelector('.phone-bar a[href="/"]')
      expect([
        home?.firstElementChild?.classList.contains('wordmark'),
        home?.getAttribute('aria-label'),
      ]).toEqual([true, 'taiyaku, Libraries'])
      // Search is the Library view's bar (spec §2), so its icon goes there.
      expect(
        host
          .querySelector('.phone-bar a[aria-label="Search"]')
          ?.getAttribute('href'),
      ).toBe('/')
    })
  })

  describe('the skip link — O174', () => {
    it('is the shell’s first element, and points at main', async () => {
      const host = await settle(
        <Shell>
          <p>x</p>
        </Shell>,
      )
      const link = host.querySelector('.library')?.firstElementChild
      expect(link?.tagName).toBe('A')
      expect(link?.textContent).toBe('Skip to content')
      expect(link?.getAttribute('href')).toBe('#content')
      const main = host.querySelector('main')
      expect(main?.id).toBe('content')
      // Not focusable until the link is used: a standing `tabindex` would
      // take focus on every click inside it.
      expect(main?.hasAttribute('tabindex')).toBe(false)
    })

    it('moves focus to main and leaves the URL alone', async () => {
      const host = await settle(
        <Shell>
          <p>x</p>
        </Shell>,
      )
      const link = host.querySelector('.skip-link')
      if (!link) throw new Error('no skip link')
      const before = window.location.href
      const click = new MouseEvent('click', { bubbles: true, cancelable: true })
      act(() => {
        link.dispatchEvent(click)
      })
      expect(click.defaultPrevented).toBe(true)
      expect(document.activeElement).toBe(host.querySelector('main'))
      const main = host.querySelector('main')
      expect(main?.getAttribute('tabindex')).toBe('-1')
      // …and only for as long as it holds that focus.
      act(() => {
        main?.blur()
      })
      expect(main?.hasAttribute('tabindex')).toBe(false)
      expect(window.location.href).toBe(before)
    })

    it('is inert while the drawer is open, as the bar and main are', async () => {
      const host = await settle(
        <Shell>
          <p>x</p>
        </Shell>,
      )
      const link = host.querySelector('.skip-link')
      expect(link?.hasAttribute('inert')).toBe(false)
      openMenu(host)
      expect(link?.hasAttribute('inert')).toBe(true)
    })
  })
})
