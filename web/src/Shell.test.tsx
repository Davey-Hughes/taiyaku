import { act } from 'react'
import { MemoryRouter } from 'react-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Me } from './api'
import { useBrowsePrefs } from './browseModule'
import { GateContext } from './gate'
import * as library from './library'
import { Shell } from './Shell'
import { useLibraryStatus } from './shell'
import { mount } from './testing/mount'
import { fakeStorage } from './testing/storage'

vi.mock('./library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./library')>()),
  status: vi.fn(),
}))

const who = (over: Partial<Me> = {}): Me => ({
  user_id: 1,
  admin: false,
  can_download: false,
  can_stream: false,
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

describe('Shell', () => {
  beforeEach(() => {
    vi.mocked(library.status).mockResolvedValue({
      roots: [{ id: 1, name: 'Manga' }],
      scanned: true,
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
   * `fetchStatus().catch` reads `err instanceof Error ? err.message :
   * String(err)`, and every other test in this file that reaches the
   * `catch` rejects with a real `Error` — so the `String(err)` half has never
   * run. A `fetch` rejection is not always an `Error`: a request `AbortController`
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
})
