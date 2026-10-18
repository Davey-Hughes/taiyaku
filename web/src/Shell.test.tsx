import { act } from 'react'
import { MemoryRouter } from 'react-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Me } from './api'
import { useBrowsePrefs } from './browseModule'
import * as curation from './curation'
import { GateContext } from './gate'
import * as library from './library'
import { Shell } from './Shell'
import { useInboxRefresh, useLibraryStatus, useStatusRefresh } from './shell'
import { deferred, flush } from './testing/async'
import { mount } from './testing/mount'
import { fakeStorage } from './testing/storage'

vi.mock('./library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./library')>()),
  status: vi.fn(),
}))

// Only the one call. `INBOX_MIN_SCORE` and `inboxTotal` are pure and are what
// the assertions below read the request and the badge through, so mocking
// them would leave the test agreeing with itself about a threshold the app
// does not use.
vi.mock('./curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./curation')>()),
  listProposals: vi.fn(),
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
 * A child that reports `continueStrip` from `BrowsePrefsContext` — the wire I2
 * pins. Reading through `useBrowsePrefs()` rather than through
 * `BrowseSettings`' own `prefs` prop matters: `BrowseSettings` is handed
 * `prefs` directly by `Shell`, so its checkbox reflects a panel change
 * regardless of what the *context* carries. Only a reader of the context
 * below `Shell` can tell a live wire from one pinned at
 * `browseModule.defaults`.
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
 * A child that asks the chrome to read the status again, on demand.
 *
 * Declared inside a function for `reporter()`'s reason. This stands in for
 * `Libraries.tsx`, which calls the same hook from its `reload`;
 * `Libraries.shell.browser.test.tsx` owns the half where a made or removed
 * library makes the call.
 */
function rereadProbe() {
  function Probe() {
    const reread = useStatusRefresh()
    return (
      <button type="button" data-testid="reread" onClick={reread}>
        reread
      </button>
    )
  }
  return <Probe />
}

/** The names the sidebar's libraries list draws, in its order. */
const sidebarLibraries = (host: HTMLElement) =>
  [...host.querySelectorAll('.sidebar-libraries a')].map((a) => a.textContent)

/** Presses `rereadProbe()`'s button and lets the read it asks for land. */
async function reread(host: HTMLElement) {
  const button = host.querySelector<HTMLElement>('[data-testid="reread"]')
  if (!button) throw new Error('no reread control rendered')
  await act(async () => {
    button.click()
    await Promise.resolve()
    await Promise.resolve()
  })
}

/** The `<label>` whose own text is exactly `label`, and the checkbox inside it. */
function checkboxLabelled(host: HTMLElement, label: string): HTMLInputElement {
  const row = [...host.querySelectorAll('.browse-settings label')].find(
    (l) => l.textContent?.trim() === label,
  )
  const box = row?.querySelector('input[type="checkbox"]')
  if (!(box instanceof HTMLInputElement)) {
    throw new Error(`no checkbox labelled "${label}"`)
  }
  return box
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
 * flips, calling its listeners the way a window resized across the
 * breakpoint would.
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
    // the admin mounts three tests above it.
    vi.clearAllMocks()
    vi.mocked(library.status).mockResolvedValue({
      libraries: [{ id: 1, name: 'Manga', scanned: true }],
      restricted: false,
    })
    vi.mocked(curation.listProposals).mockResolvedValue({
      proposals: [],
      counts: { 'edition-of': 5, 'same-series': 2 },
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

  it('offers the theme switch at the foot of the sidebar', async () => {
    vi.stubGlobal('localStorage', fakeStorage())
    const host = await settle(<Shell>{null}</Shell>)
    const column = host.querySelector('.sidebar-column')
    expect(column?.lastElementChild?.classList.contains('theme-control')).toBe(
      true,
    )
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
   * The list's switch went with the preference it set: a `browse`-module
   * key is a control in this panel, so a key left behind would draw a box
   * that switches nothing.
   */
  it('offers no switch for the libraries list in the settings', async () => {
    const host = await settle(<Shell>x</Shell>, who({ admin: true }))
    const labels = [...host.querySelectorAll('.browse-settings label')].map(
      (l) => l.textContent?.trim(),
    )
    expect(labels).toContain('continue strip')
    expect(labels).not.toContain('show roots')
  })

  /**
   * `Sidebar.tsx`'s proposals link is admin-only, and `Sidebar` itself only
   * knows that from its `admin` prop — nothing about the sidebar's own tests
   * can pin that `Shell` actually threads the gate's `admin` bit into that
   * prop, since `Sidebar.test.tsx` constructs `<Sidebar>` directly. Without
   * this, deleting the `admin={admin}` line `Shell.tsx` passes down would
   * leave every admin account seeing no link, silently, and no test in this
   * suite or `Sidebar.test.tsx` would fail.
   */
  it('gives the sidebar the account’s admin bit, so the proposals link follows it too', async () => {
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
   * O94: the account link is the one sidebar entry that follows no account's
   * role. `Sidebar.test.tsx` pins it against the prop; this pins it through
   * the chrome a signed-in reader actually gets, with the gate's `admin` bit
   * false — so a later change that gated it on anything `Shell` knows about
   * the account is red here as well as there.
   */
  it('offers a signed-in reader the account screen', async () => {
    const asReader = await settle(
      <Shell>
        <p>x</p>
      </Shell>,
      who({ admin: false }),
    )
    expect(
      asReader.querySelector('.sidebar a[href="/account"]')?.textContent,
    ).toBe('Account')
  })

  /**
   * The count itself. `Sidebar.test.tsx` pins that a number handed to
   * `Sidebar` is drawn; nothing there can pin that `Shell` fetches one, or
   * *which* one — deleting this effect leaves every sidebar test green and
   * every deployment back to a bare `Proposals` entry, which is exactly how
   * the entry shipped without a count in the first place.
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
   * only be the `?? false` default, not a sidebar that never drew.
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
   * `fetchStatus().catch` reads `message.ts`'s `messageOf`, and every other
   * test in this file that reaches the `catch` rejects with a real `Error` —
   * so its `String(err)` half runs here and nowhere else on this screen. A
   * `fetch` rejection is not always an `Error`: a request `AbortController`
   * cancels, or a mock in another test, can reject with anything at all, and
   * this is what the reader sees when it does.
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
  it('re-reads the status when a screen under it says a write moved the libraries', async () => {
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
   * `density` decides the one class name `Shell` puts on its own root —
   * `'comfortable'`, every other test's default, never reaches the ternary's
   * `'compact'` branch. Seeded through storage rather than through
   * `BrowseSettings`' own control, because this is about what `Shell` draws
   * from a stored preference on first mount, not about the panel that writes
   * one.
   */
  it('marks the root compact when the stored preference says so', async () => {
    const s = fakeStorage()
    s.setItem('taiyaku.prefs.browse', JSON.stringify({ density: 'compact' }))
    vi.stubGlobal('localStorage', s)
    const host = await settle(
      <Shell>
        <p>x</p>
      </Shell>,
    )
    expect(host.querySelector('.library.compact')).not.toBeNull()
  })

  /**
   * `module.ts`'s `sliceReset` doc: `resetPrefs` alone satisfies "reload,
   * confirm it persisted" and silently fails "change a setting, confirm it
   * takes effect" — the in-memory half alone is the mirror failure, a reload
   * resurrecting the old value. Neither half proves the other, so this pins
   * the one `BrowseSettings.test.tsx` cannot: `resetPrefs(browseModule.id)`
   * actually clearing `localStorage`, not just `reset()`'s in-memory
   * `sliceReset` making the panel agree with itself.
   *
   * A fresh `mount()` is what stands in for a reload — a second render of the
   * same instance would only re-read the state already in memory, which is
   * exactly the half this test must not credit.
   */
  it('reset clears storage, so a fresh mount does not resurrect the changed value', async () => {
    vi.stubGlobal('localStorage', fakeStorage())

    const before = await settle(
      <Shell>
        <p>x</p>
      </Shell>,
    )
    const stripBefore = checkboxLabelled(before, 'continue strip')
    expect(stripBefore.checked).toBe(true)
    act(() => {
      stripBefore.click()
    })
    expect(stripBefore.checked).toBe(false)

    const resetButton = [
      ...before.querySelectorAll('.browse-settings button'),
    ].find((b) => b.textContent === 'Reset')
    if (!(resetButton instanceof HTMLButtonElement)) {
      throw new Error('the settings panel has no Reset button')
    }
    act(() => {
      resetButton.click()
    })
    before.remove()

    // A fresh mount, standing in for a reload: `loadPrefs` reads whatever
    // `resetPrefs` left in storage, not the map still sitting in the first
    // instance's memory.
    const after = await settle(
      <Shell>
        <p>x</p>
      </Shell>,
    )
    expect(checkboxLabelled(after, 'continue strip').checked).toBe(true)
  })

  /**
   * I2 of the whole-branch review: `BrowsePrefsContext.Provider` is the one
   * wire from `Shell`'s own `prefs` state to every browse screen mounted
   * below it, and nothing pinned that the value handed to `value={…}` is
   * `prefs` itself rather than some constant that merely has the right
   * shape. `module.ts`'s `sliceReset` doc names the missing test outright:
   * "change a setting, confirm it takes effect" — written here at the
   * `Shell` level, through a probe that reads the context the way a real
   * screen (`Series`, `SeriesGrid`'s caller) would, rather than through
   * `BrowseSettings`' own `prefs` prop, which `Shell` still hands the panel
   * directly and so would reflect the change regardless of whether the
   * context wire itself works.
   */
  it('threads a changed preference from the settings panel to whatever reads the context below it', async () => {
    const host = await settle(<Shell>{continueStripProbe()}</Shell>)
    expect(
      host.querySelector('[data-testid="continue-strip"]')?.textContent,
    ).toBe('true')

    const strip = checkboxLabelled(host, 'continue strip')
    expect(strip.checked).toBe(true)
    act(() => {
      strip.click()
    })

    expect(
      host.querySelector('[data-testid="continue-strip"]')?.textContent,
    ).toBe('false')
  })

  /**
   * I3 of the whole-branch review: the reset test above pins the mirror
   * case — that a reload does *not* resurrect a value nothing ever stored —
   * which a deleted `persistPrefs` call satisfies for free, since with
   * nothing ever written a fresh mount reads defaults either way. This is
   * the direction that mutation actually breaks: a preference the reader
   * *changed* has to still be there after a reload. A fresh `mount()`
   * stands in for the reload, same as the reset test's `before`/`after`
   * pair.
   */
  it('a changed preference survives a reload, not just a reset', async () => {
    vi.stubGlobal('localStorage', fakeStorage())

    const before = await settle(
      <Shell>
        <p>x</p>
      </Shell>,
    )
    const stripBefore = checkboxLabelled(before, 'continue strip')
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
        <p>x</p>
      </Shell>,
    )
    expect(checkboxLabelled(after, 'continue strip').checked).toBe(false)
  })

  /**
   * The drawer — spec 2026-09-17 §7.1. jsdom lays nothing out, so this pins
   * the state and the attributes; `Shell.browser.test.tsx` measures what a
   * browser does with them: where Tab can go, whether the page scrolls
   * behind the drawer, where the column sits.
   */
  describe('the menu below 900 px', () => {
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

    it('takes focus to its first link, and gives it back to ☰ on Escape', async () => {
      const host = await settle(
        <Shell>
          <p>x</p>
        </Shell>,
      )
      openMenu(host)
      expect(document.activeElement).toBe(
        menu(host).querySelector('a[href="/"]'),
      )
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
      const search =
        menu(host).querySelector<HTMLAnchorElement>('a[href="/search"]')
      if (!search) throw new Error('no search link in the menu')
      await act(async () => {
        search.click()
      })
      expect(menuButton(host).getAttribute('aria-expanded')).toBe('false')
    })

    it('closes when the window widens past the breakpoint, where the column is the sidebar again', async () => {
      const media = fakeMatchMedia()
      vi.stubGlobal('matchMedia', media.matchMedia)
      const host = await settle(
        <Shell>
          <p>x</p>
        </Shell>,
      )
      openMenu(host)
      expect(media.matchMedia).toHaveBeenCalledWith('(width >= 900px)')
      act(() => {
        media.widen()
      })
      expect(menuButton(host).getAttribute('aria-expanded')).toBe('false')
    })

    it('draws the wordmark at the top of the column, and as the bar’s way home', async () => {
      const host = await settle(
        <Shell>
          <p>x</p>
        </Shell>,
      )
      expect(menu(host).firstElementChild?.classList.contains('wordmark')).toBe(
        true,
      )
      expect(
        host.querySelector('.phone-bar a[href="/"] .wordmark'),
      ).not.toBeNull()
      expect(
        host
          .querySelector('.phone-bar a[href="/search"]')
          ?.getAttribute('aria-label'),
      ).toBe('Search')
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
