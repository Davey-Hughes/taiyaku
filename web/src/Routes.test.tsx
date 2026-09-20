import { act, useEffect } from 'react'
import { MemoryRouter, useNavigate, useParams } from 'react-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import * as api from './api'
import { GateContext } from './gate'
import * as library from './library'
import { AppRoutes } from './Routes'
import { flush } from './testing/async'
import { mount } from './testing/mount'

// Populated by the mocked `App` below, from a `useEffect` with an empty
// dependency array — so it records one entry per *mount*, not per render.
// `vi.hoisted` runs during the same hoisting pass `vi.mock`'s factories do,
// which is what makes this safe to close over from inside the factory (see
// `App.test.tsx`'s `ports` for the same pattern).
const { mounts } = vi.hoisted(() => ({ mounts: [] as string[] }))

// Wrapped rather than replaced: every other test in this file needs
// `useParams` to keep answering for real, off the URL `MemoryRouter` was
// given — only the "no :work" test below overrides one call. `matchRoutes`
// (what actually matches a path against `path="/read/:work"`) does not go
// through this hook, so wrapping it cannot change which element mounts for
// a given URL, only what `Reader` reads once it has.
vi.mock('react-router', async (importOriginal) => {
  const actual = await importOriginal<typeof import('react-router')>()
  return { ...actual, useParams: vi.fn(actual.useParams) }
})

vi.mock('./App', () => ({
  default: ({ work, onLeave }: { work: string; onLeave: () => void }) => {
    // Empty deps: fires once per mount, never on a prop-only update — which is
    // the whole point. A reused instance (the unpinned-`key` regression) would
    // push nothing a second time.
    // biome-ignore lint/correctness/useExhaustiveDependencies: `work` is read once per mount, deliberately — tracking it would fire this effect on every prop-only update too, which is exactly the case this test needs to tell apart from a genuine remount.
    useEffect(() => {
      mounts.push(work)
    }, [])
    return (
      <div data-testid="reader">
        {work}
        <button type="button" data-testid="leave" onClick={onLeave}>
          leave
        </button>
      </div>
    )
  },
}))
vi.mock('./library', () => ({
  home: vi.fn().mockResolvedValue({ continuing: [], added: [], read: [] }),
  grid: vi.fn().mockResolvedValue({ series: [], unfiled: [] }),
  status: vi.fn().mockResolvedValue({ roots: [], scanned: true }),
  matchesChip: () => true,
  sorted: (s: unknown[]) => s,
  // Task 4 wires `/series/:id` to `Series`, which fetches `oneSeries` on
  // mount — a route this file exercises (`/series/1`, below). Its own
  // behaviour is `Series.test.tsx`'s job; this fixture only has to resolve
  // so the route mounts without an unhandled rejection.
  oneSeries: vi.fn().mockResolvedValue({
    series: {
      id: 1,
      title: 'Series',
      works: 0,
      editions: 0,
      aligned: 0,
      added: 0,
      unread: 0,
      cover: '',
    },
    works: [],
  }),
  // Task 10 wires `/search` to `Search`, which calls `search` once the
  // debounce settles on the `?q=` value carried in from the URL — a route
  // this file exercises (`/search?q=hello`, below). Its own behaviour is
  // `Search.test.tsx`'s job; this fixture only has to resolve so the route
  // mounts without an unhandled rejection, the same reason `oneSeries` above
  // exists.
  search: vi
    .fn()
    .mockResolvedValue({ series: [], unfiled: [], query: 'hello' }),
}))
// Task 7 wires `/proposals` to `Proposals`, which fetches `listProposals` on
// mount — a route this file exercises (`/proposals`, below). Its own
// behaviour is `Proposals.test.tsx`'s job; this fixture only has to resolve
// so the route mounts without an unhandled rejection, the same reason
// `oneSeries` and `search` above exist.
vi.mock('./curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./curation')>()),
  listProposals: vi.fn().mockResolvedValue({ proposals: [], counts: {} }),
}))
// O94 wires `/account` to `Account`, which lists this account's keys and
// sessions on mount — a route this file exercises (`/account`, below). Its own
// behaviour is `Account.test.tsx`'s job; these only have to resolve, the same
// reason `oneSeries`, `search` and `listProposals` above exist. `logout` is
// for the sign-out test, which belongs here because the arrow handing the
// gate's `signOut` to `Account` is `Routes.tsx`'s.
vi.mock('./api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./api')>()),
  listKeys: vi.fn().mockResolvedValue([]),
  listSessions: vi.fn().mockResolvedValue([]),
  logout: vi.fn().mockResolvedValue(undefined),
}))

// `Shell` and `Library` both fetch on mount, and every mock above resolves at
// once — so a test that lands on the library and asserts straight after `at()`
// still has those answers arriving the moment it returns, outside `act`, where
// React reports "An update to Library inside a test was not wrapped in
// act(...)". Such a test ends in `flush()`: nothing it asserts depends on the
// answers, and the flush only moves them inside `act`.
const at = (path: string) =>
  mount(
    <MemoryRouter initialEntries={[path]}>
      <AppRoutes />
    </MemoryRouter>,
  )

describe('the routes', () => {
  beforeEach(() => {
    mounts.length = 0
  })

  // Only the search route below turns fake timers on — `Search`'s debounce
  // needs them to settle deterministically. Every other test here runs on
  // real timers, so this has to come back off after, not just get left on
  // for whichever test happens to run next in the file.
  afterEach(() => {
    vi.useRealTimers()
  })

  it('opens the reader on the work in the URL', () => {
    const host = at('/read/gotoubun-v01')
    // `toContain` rather than `toBe`: the mocked `App` also renders the
    // `onLeave` button (asserted below), so the reader's textContent is more
    // than just the slug now.
    expect(host.querySelector('[data-testid="reader"]')?.textContent).toContain(
      'gotoubun-v01',
    )
  })

  it('puts the reader inside its route box, which blacks out the page before the reader draws', () => {
    const host = at('/read/gotoubun-v01')
    expect(
      host.querySelector('.reader-route > [data-testid="reader"]'),
    ).not.toBeNull()
  })

  it('does not mount the reader anywhere else', async () => {
    const host = at('/')
    // Negative half: the reader is absent. On its own this would also pass
    // if the router rendered nothing at all, so the positive half below —
    // the library actually rendered — is what tells "the library is here
    // instead" from "nothing is here".
    expect(host.querySelector('[data-testid="reader"]')).toBeNull()
    expect(host.querySelector('.library')).not.toBeNull()
    expect(host.querySelector('.sidebar')).not.toBeNull()
    await flush()
  })

  it('answers an unknown path with the library rather than the reader', async () => {
    const host = at('/nonsense')
    // Asserted structurally rather than by text: the `Library` this task
    // mounts is Task 8's placeholder and has no text content yet.
    expect(host.querySelector('main')).not.toBeNull()
    expect(host.querySelector('[data-testid="reader"]')).toBeNull()
    await flush()
  })

  it('routes a series id into the series page inside the shell', async () => {
    // `.library`/`.sidebar` alone come from `Shell`, which wraps every route
    // in this file — they would still be present if `/series/:id` rendered
    // `<Shell><Library /></Shell>` by mistake, so on their own they cannot
    // tell `Series` apart from any other child. `oneSeries` having been
    // called with the id parsed out of the URL is something only `Series`
    // produces; `Series.test.tsx` owns the rest of that component's content.
    const host = at('/series/1')
    await act(async () => {
      await Promise.resolve()
    })
    expect(host.querySelector('[data-testid="reader"]')).toBeNull()
    expect(host.querySelector('.library')).not.toBeNull()
    expect(host.querySelector('.sidebar')).not.toBeNull()
    expect(vi.mocked(library.oneSeries)).toHaveBeenCalledWith(1)
  })

  /**
   * The same trap "routes a series id into the series page inside the shell"
   * above already had to avoid: `.library`/`.sidebar` alone come from `Shell`,
   * which wraps every route in this file, so they would still be present if
   * `/search` rendered `<Shell><Library /></Shell>` by mistake. A search box
   * is something only `Search` renders, and `library.search` having been
   * called with the `?q=` value parsed out of the URL is something only its
   * own effect produces; `Search.test.tsx` owns the rest of that component's
   * content.
   */
  it('routes a search into the search screen inside the shell', async () => {
    vi.useFakeTimers()
    const host = at('/search?q=hello')
    expect(host.querySelector('input[type="search"]')).not.toBeNull()
    await act(async () => {
      vi.advanceTimersByTime(300)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('[data-testid="reader"]')).toBeNull()
    expect(host.querySelector('.library')).not.toBeNull()
    expect(host.querySelector('.sidebar')).not.toBeNull()
    expect(vi.mocked(library.search)).toHaveBeenCalledWith('hello')
  })

  /**
   * Same trap as the two tests above: `.library`/`.sidebar` alone come from
   * `Shell`, which wraps every route in this file, so they would still be
   * present if `/proposals` rendered `<Shell><Library /></Shell>` by mistake.
   * `.proposals` is a class only `Proposals` itself renders.
   */
  it('routes /proposals into the proposals screen inside the shell', async () => {
    const host = at('/proposals')
    await act(async () => {
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('[data-testid="reader"]')).toBeNull()
    expect(host.querySelector('.library')).not.toBeNull()
    expect(host.querySelector('.sidebar')).not.toBeNull()
    expect(host.querySelector('.proposals')).not.toBeNull()
  })

  /**
   * O94. The same trap again, so the screen is found by what only `Account`
   * draws — its labelled section, directly inside the shell's `<main>`, and
   * the current-password box — and by `listKeys`, which only its effect asks.
   * **The library this file mocks holds no work at all**: `grid` answers no
   * series and nothing unfiled, and `status` no roots. That is the row's own
   * case — `Account` used to mount only inside `/read/:work`, so an account
   * in a library with nothing readable had no way to it.
   */
  it('routes /account into the account screen inside the shell, in a library with no works', async () => {
    const host = at('/account')
    await flush()
    expect(host.querySelector('[data-testid="reader"]')).toBeNull()
    expect(host.querySelector('.sidebar')).not.toBeNull()
    const screen = host.querySelector(
      '.library > main > section[aria-label="Account"]',
    )
    expect(screen).not.toBeNull()
    expect(
      screen?.querySelector('input[autocomplete="current-password"]'),
    ).not.toBeNull()
    expect(host.querySelector('[role="dialog"]')).toBeNull()
    expect(vi.mocked(api.listKeys)).toHaveBeenCalled()
  })

  it('signs out from /account through the gate above the routes', async () => {
    const signOut = vi.fn()
    const host = mount(
      <MemoryRouter initialEntries={['/account']}>
        <GateContext
          value={{
            who: {
              user_id: 2,
              admin: false,
              can_download: false,
              can_stream: false,
              can_stamp: false,
            },
            signOut,
          }}
        >
          <AppRoutes />
        </GateContext>
      </MemoryRouter>,
    )
    await flush()
    const button = [...host.querySelectorAll('.library > main button')].find(
      (b) => b.textContent === 'Sign out',
    )
    if (!(button instanceof HTMLButtonElement)) {
      throw new Error('no Sign out button on the account screen')
    }
    await act(async () => {
      button.click()
      await Promise.resolve()
    })
    await flush()
    expect(signOut).toHaveBeenCalledTimes(1)
  })

  it('sends a reader route with no :work back to the library rather than crashing', async () => {
    // Nothing in this route table can actually put `Reader` on screen with
    // `work` missing: `path="/read/:work"` makes the segment mandatory, so
    // e.g. `matchPath('/read/:work', '/read/')` is `null` and that URL falls
    // through to the catch-all route instead (see "answers an unknown path"
    // above) — `Reader` never even mounts. `useParams()` is still typed to
    // answer `{ work: undefined }`, so `Reader`'s guard exists against that
    // hook's own contract rather than against this route table, and is
    // exercised the same way: by answering it that once, directly.
    vi.mocked(useParams).mockReturnValueOnce({})
    const host = at('/read/gotoubun-v01')
    expect(host.querySelector('[data-testid="reader"]')).toBeNull()
    expect(host.querySelector('.library')).not.toBeNull()
    await flush()
  })

  it('leaving the reader navigates back to the library', async () => {
    const host = at('/read/gotoubun-v01')
    expect(host.querySelector('[data-testid="reader"]')).not.toBeNull()
    // The mocked App exposes its `onLeave` as a button.
    await act(async () => {
      host.querySelector<HTMLButtonElement>('[data-testid="leave"]')?.click()
      await Promise.resolve()
    })
    expect(host.querySelector('[data-testid="reader"]')).toBeNull()
  })

  /**
   * `Routes.tsx`'s own doc calls `key={work}` "the whole reason `App.tsx` did
   * not have to become re-entrant" — but nothing before this test ever
   * navigated this file from one `/read/:work` URL to another, so nothing
   * here actually pinned that the key does its job. `App.test.tsx`'s
   * "a changing work remounts" test proves React's key semantics in general,
   * with `rerender` supplying `key="one"`/`key="two"` itself; it says nothing
   * about whether `Reader` actually keys `<App>` by the URL segment.
   *
   * Navigation happens through a real `useNavigate()`, not a second `mount()`
   * — two separate `mount()` calls would each get a fresh `App` instance
   * regardless of any `key`, so that could not tell a remount from an update
   * either.
   */
  it('navigating from one work to another remounts the reader rather than reusing it', async () => {
    let navigate: ReturnType<typeof useNavigate> | undefined
    function Nav() {
      navigate = useNavigate()
      return <AppRoutes />
    }
    mount(
      <MemoryRouter initialEntries={['/read/one']}>
        <Nav />
      </MemoryRouter>,
    )
    expect(mounts).toEqual(['one'])

    await act(async () => {
      navigate?.('/read/two')
      await Promise.resolve()
    })
    // Two mounts, not one: without `key={work}`, React reuses the existing
    // `App` instance across the URL change and this effect never fires a
    // second time.
    expect(mounts).toEqual(['one', 'two'])
  })
})
