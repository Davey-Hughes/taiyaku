import { act } from 'react'
import { MemoryRouter } from 'react-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Me } from './api'
import { useBrowsePrefs } from './browseModule'
import * as curation from './curation'
import { GateContext } from './gate'
import * as library from './library'
import { Shell } from './Shell'
import { useInboxRefresh, useLibraryStatus } from './shell'
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
    return <p data-testid="roots">{status ? status.roots.length : 'pending'}</p>
  }
  return <Reporter />
}

/**
 * A child that reports `showRoots` from `BrowsePrefsContext` — the wire I2
 * pins. Reading through `useBrowsePrefs()` rather than through
 * `BrowseSettings`' own `prefs` prop matters: `BrowseSettings` is handed
 * `prefs` directly by `Shell`, so its checkbox reflects a panel change
 * regardless of what the *context* carries. Only a reader of the context
 * below `Shell` can tell a live wire from one pinned at
 * `browseModule.defaults`.
 */
function showRootsProbe() {
  function Probe() {
    const prefs = useBrowsePrefs()
    return <p data-testid="show-roots">{String(prefs.showRoots)}</p>
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
      roots: [{ id: 1, name: 'Manga' }],
      scanned: true,
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
    expect(host.querySelector('[data-testid="roots"]')?.textContent).toBe('1')
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

  it('draws the roots list for an admin and not for a reader', async () => {
    const asAdmin = await settle(
      <Shell>
        <p>x</p>
      </Shell>,
      who({ admin: true }),
    )
    expect(asAdmin.querySelector('.sidebar-roots')).not.toBeNull()

    const asReader = await settle(
      <Shell>
        <p>x</p>
      </Shell>,
      who({ admin: false }),
    )
    expect(asReader.querySelector('.sidebar-roots')).toBeNull()
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
   * this is the only test in this file — or, after Task 3, in this suite —
   * that mounts with no gate above it *and* looks at `.sidebar-roots`: the
   * "draws the roots list…" test above always mounts with a provider
   * present, and this file's other no-provider tests assert on
   * `[data-testid="roots"]` or `[role="alert"]`, never on the sidebar.
   *
   * The roots list resolves to a non-empty array here (the `beforeEach`
   * default), and the reporter child proves it landed before the sidebar
   * assertion runs — so an absent `.sidebar-roots` below can only be the
   * `?? false` default, not an accident of there being nothing to draw.
   */
  it('draws no roots list where no gate sits above it, even though roots are there to draw', async () => {
    const host = await settle(<Shell>{reporter()}</Shell>)
    expect(host.querySelector('[data-testid="roots"]')?.textContent).toBe('1')
    expect(host.querySelector('.sidebar-roots')).toBeNull()
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
    const showRootsBefore = checkboxLabelled(before, 'show roots')
    expect(showRootsBefore.checked).toBe(false)
    act(() => {
      showRootsBefore.click()
    })
    expect(showRootsBefore.checked).toBe(true)

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
    expect(checkboxLabelled(after, 'show roots').checked).toBe(false)
  })

  /**
   * I4 + M12 of the whole-branch review: the comment above `reset` used to
   * claim the admin fold is "a first-run default, not a floor" — but Reset
   * did not survive a reload for an admin. `resetPrefs` clears storage, and
   * the next mount's `admin` fold runs against that *absent* storage and puts
   * `showRoots` right back to `true`, because the old `setMap` here persisted
   * from inside its own updater rather than from an effect and so never
   * re-persisted `sliceReset`'s result. `hooks.ts`'s `usePrefsMap` — the same
   * hook `App.tsx` uses for the reader's own modules — persists from an
   * effect instead, which re-runs after `sliceReset` and writes `showRoots:
   * false` back to storage before any reload can see it. This is the case
   * `checkboxLabelled` above only proves half of: that test never reloads as
   * an admin, so it could not have caught the fold winning back.
   */
  it('as an admin, Reset survives a reload — show roots stays off', async () => {
    vi.stubGlobal('localStorage', fakeStorage())

    const before = await settle(
      <Shell>
        <p>x</p>
      </Shell>,
      who({ admin: true }),
    )
    expect(
      before.querySelector('.sidebar-roots'),
      'admin fold on first run',
    ).not.toBeNull()

    const resetButton = [
      ...before.querySelectorAll('.browse-settings button'),
    ].find((b) => b.textContent === 'Reset')
    if (!(resetButton instanceof HTMLButtonElement)) {
      throw new Error('the settings panel has no Reset button')
    }
    act(() => {
      resetButton.click()
    })
    expect(
      before.querySelector('.sidebar-roots'),
      'in-app, right after Reset',
    ).toBeNull()
    before.remove()

    // A fresh mount, standing in for a reload — still as an admin, which is
    // exactly the case the in-app confirmation and the persisted result
    // disagreed on.
    const after = await settle(
      <Shell>
        <p>x</p>
      </Shell>,
      who({ admin: true }),
    )
    expect(after.querySelector('.sidebar-roots'), 'after the reload').toBeNull()
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
    const host = await settle(<Shell>{showRootsProbe()}</Shell>)
    expect(host.querySelector('[data-testid="show-roots"]')?.textContent).toBe(
      'false',
    )

    const showRoots = checkboxLabelled(host, 'show roots')
    expect(showRoots.checked).toBe(false)
    act(() => {
      showRoots.click()
    })

    expect(host.querySelector('[data-testid="show-roots"]')?.textContent).toBe(
      'true',
    )
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
    const showRootsBefore = checkboxLabelled(before, 'show roots')
    expect(showRootsBefore.checked).toBe(false)
    act(() => {
      showRootsBefore.click()
    })
    expect(showRootsBefore.checked).toBe(true)
    before.remove()

    // A fresh mount, standing in for a reload: `loadPrefs` reads whatever
    // `usePrefsMap`'s persist effect actually wrote, not the map still
    // sitting in the first instance's memory.
    const after = await settle(
      <Shell>
        <p>x</p>
      </Shell>,
    )
    expect(checkboxLabelled(after, 'show roots').checked).toBe(true)
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
