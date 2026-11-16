import { act, useEffect } from 'react'
import { MemoryRouter, useLocation, useNavigate, useParams } from 'react-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import * as api from '../src/api'
import { GateContext } from '../src/gate'
import * as library from '../src/library'
import { AppRoutes } from '../src/Routes'
import { flush } from './support/async'
import { licenceTextsLanded } from './support/licence'
import { mount } from './support/mount'

// Populated by the mocked `App` below, from a `useEffect` with an empty
// dependency array — so it records one entry per *mount*, not per render.
// `vi.hoisted` runs during the same hoisting pass `vi.mock`'s factories do,
// which is what makes this safe to close over from inside the factory (see
// `App.test.tsx`'s `ports` for the same pattern). `onMount.current` is called
// with `work` from the same effect, so a test can do what the real `App` does
// with the slug it is handed; it is a no-op unless a test sets it.
const { mounts, onMount } = vi.hoisted(() => ({
  mounts: [] as string[],
  onMount: { current: (_work: string): void => {} },
}))

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

vi.mock('../src/App', () => ({
  default: ({ work, onLeave }: { work: string; onLeave: () => void }) => {
    // Empty deps: fires once per mount, never on a prop-only update — which is
    // the whole point. A reused instance (the unpinned-`key` regression) would
    // push nothing a second time.
    // biome-ignore lint/correctness/useExhaustiveDependencies: `work` is read once per mount, deliberately — tracking it would fire this effect on every prop-only update too, which is exactly the case this test needs to tell apart from a genuine remount.
    useEffect(() => {
      mounts.push(work)
      onMount.current(work)
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
vi.mock('../src/BookReader', () => ({
  BookReader: ({ work, onLeave }: { work: string; onLeave: () => void }) => (
    <div data-testid="book">
      {work}
      <button type="button" data-testid="leave-book" onClick={onLeave}>
        leave
      </button>
    </div>
  ),
}))
vi.mock('../src/library', async (importOriginal) => ({
  // The real parser of `?library=`: it is a pure function over the URL, and
  // the library page and the sidebar both read the parameter through it.
  libraryParam: (await importOriginal<typeof import('../src/library')>())
    .libraryParam,
  // The real cover URLs, a pure function too, for the tile a search's hit
  // draws (`a link to /search`, below).
  coverSources: (await importOriginal<typeof import('../src/library')>())
    .coverSources,
  // The real readers and writers of the Library view's address, pure
  // functions too: the view and the sidebar beside it read `?show=` and
  // `?sort=` through them, and the view's reads name their addresses with
  // `narrowedTo`.
  viewIn: (await importOriginal<typeof import('../src/library')>()).viewIn,
  withView: (await importOriginal<typeof import('../src/library')>()).withView,
  narrowedTo: (await importOriginal<typeof import('../src/library')>())
    .narrowedTo,
  home: vi.fn().mockResolvedValue({ continuing: [], added: [], read: [] }),
  grid: vi.fn().mockResolvedValue({ series: [], unfiled: [] }),
  status: vi.fn().mockResolvedValue({ libraries: [] }),
  matchesChip: () => true,
  sorted: (s: unknown[]) => s,
  // `/series/:id` is wired to `Series`, which fetches `oneSeries` on mount — a
  // route this file exercises (`/series/1`, below). Its own behaviour is
  // `Series.test.tsx`'s job; this fixture only has to resolve so the route
  // mounts without an unhandled rejection.
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
  // `/search` lands on the Library view, whose search bar calls `search`
  // once the debounce settles on the `?q=` carried in from the URL — a route
  // this file exercises (`/search?q=hello`, below). The bar's own behaviour
  // is `Library.test.tsx`'s and `searchHits.test.tsx`'s; this fixture only
  // has to resolve so the route mounts without an unhandled rejection, the
  // same reason `oneSeries` above exists.
  search: vi
    .fn()
    .mockResolvedValue({ series: [], unfiled: [], query: 'hello' }),
}))
// `/proposals` is wired to `Proposals`, which fetches `listProposals` on
// mount — a route this file exercises (`/proposals`, below). Its own
// behaviour is `Proposals.test.tsx`'s job; this fixture only has to resolve
// so the route mounts without an unhandled rejection, the same reason
// `oneSeries` and `search` above exist.
vi.mock('../src/curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/curation')>()),
  listProposals: vi.fn().mockResolvedValue({ proposals: [], counts: {} }),
}))
// `/account` is wired to `Account` (O94), which lists this account's keys and
// sessions on mount — a route this file exercises (`/account`, below). Its own
// behaviour is `Account.test.tsx`'s job; these only have to resolve, the same
// reason `oneSeries`, `search` and `listProposals` above exist. `logout` is
// for the sign-out test, which belongs here because the arrow handing the
// gate's `signOut` to `Account` is `Routes.tsx`'s.
vi.mock('../src/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/api')>()),
  listKeys: vi.fn().mockResolvedValue([]),
  listSessions: vi.fn().mockResolvedValue([]),
  logout: vi.fn().mockResolvedValue(undefined),
}))

// `/about` draws `About`, which asks for the build on mount, as the label at
// every shell's foot does. What both draw is `About.test.tsx`'s and
// `VersionLabel.test.tsx`'s; this only has to resolve.
vi.mock('../src/about', async (importOriginal) => {
  const { ABOUT } = await import('./support/about')
  return {
    ...(await importOriginal<typeof import('../src/about')>()),
    about: vi.fn().mockResolvedValue(ABOUT),
  }
})

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

  // Only the search routes below turn fake timers on — the search bar's
  // debounce needs them to settle deterministically. Every other test here
  // runs on real timers, so this has to come back off after, not just get
  // left on for whichever test happens to run next in the file.
  afterEach(() => {
    vi.useRealTimers()
    vi.unstubAllGlobals()
    onMount.current = () => {}
  })

  it('opens the reader on the work in the URL', () => {
    const host = at('/read/gotoubun-v01')
    // `toContain` rather than `toBe`: the mocked `App` also renders the
    // `onLeave` button (asserted below), so the reader's textContent is more
    // than just the slug.
    expect(host.querySelector('[data-testid="reader"]')?.textContent).toContain(
      'gotoubun-v01',
    )
  })

  /**
   * O262: a slug outside ASCII arrives percent-encoded, as a browser delivers
   * the path, and the reader asks the server for it encoded once: the router
   * hands `App` the decoded slug, and `api.ts`'s `seg` encodes it for the
   * request. Encoded again or not at all, the one request names another work.
   */
  it('asks the server for a slug outside ASCII encoded once — O262', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(new Response('{"taiyaku":1}', { status: 200 }))
    vi.stubGlobal('fetch', fetchMock)
    onMount.current = (work) => {
      void api.getSidecarWithEtag(work)
    }
    at('/read/lumi%C3%A8re-v02')
    await flush()
    expect(fetchMock.mock.calls).toEqual([['/api/works/lumi%C3%A8re-v02']])
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
    // Asserted structurally rather than by text.
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
    expect(vi.mocked(library.oneSeries)).toHaveBeenLastCalledWith(1, undefined)
  })

  it('routes a series opened from a library into the series page with that library — spec §3.6', async () => {
    at('/series/1?library=2')
    await act(async () => {
      await Promise.resolve()
    })
    expect(vi.mocked(library.oneSeries)).toHaveBeenLastCalledWith(1, 2)
  })

  /**
   * `/search` redirects to the Library view with its query and its library,
   * so a search link keeps working (spec §2). The hits are what show the
   * view took the query from the URL: the box is the view's own, so its
   * presence alone says nothing of the redirect.
   */
  describe('a link to /search', () => {
    beforeEach(() => {
      vi.useFakeTimers()
      vi.mocked(library.search).mockClear()
    })

    /** The routes at `path`, with the location they settle on beside them. */
    const settledAt = async (path: string) => {
      // Declared here for the reason `Shell.test.tsx`'s `reporter()` gives.
      function Probe() {
        const { pathname, search } = useLocation()
        return <output data-testid="location">{pathname + search}</output>
      }
      const host = mount(
        <MemoryRouter initialEntries={[path]}>
          <AppRoutes />
          <Probe />
        </MemoryRouter>,
      )
      // The shell's status first: the view runs a query only once it knows
      // there is a library to search. Then the debounce.
      await flush()
      await act(async () => {
        vi.advanceTimersByTime(300)
        await Promise.resolve()
        await Promise.resolve()
      })
      return host
    }

    const locationOf = (host: HTMLElement) =>
      host.querySelector('[data-testid="location"]')?.textContent

    it('lands on the Library view with its query and its library, and shows the hits', async () => {
      vi.mocked(library.search).mockResolvedValueOnce({
        series: [
          {
            id: 3,
            title: 'Hello Series',
            works: 1,
            editions: 1,
            aligned: 0,
            added: 0,
            unread: 1,
            cover: '',
          },
        ],
        unfiled: [],
        query: 'hello',
      })
      // A library this account sees: the view searches only a library its
      // grid can show.
      vi.mocked(library.status).mockResolvedValueOnce({
        libraries: [{ id: 2, name: 'Comics', scanned: true }],
        restricted: false,
      })
      const host = await settledAt('/search?q=hello&library=2')
      expect(locationOf(host)).toBe('/?q=hello&library=2')
      expect(host.querySelector('.library')).not.toBeNull()
      expect(host.querySelector('.sidebar')).not.toBeNull()
      expect(
        host.querySelector<HTMLInputElement>('input[type="search"]')?.value,
      ).toBe('hello')
      expect(vi.mocked(library.search).mock.calls).toEqual([['hello', 2]])
      expect(host.querySelector('.search-hits .grid .title')?.textContent).toBe(
        'Hello Series',
      )
      expect(
        host.querySelector('.search-hits .grid a')?.getAttribute('href'),
      ).toBe('/series/3?library=2')
    })

    it('leaves an empty library behind, so the search is of every library', async () => {
      // A server with a library: on one with none the view runs no query.
      vi.mocked(library.status).mockResolvedValueOnce({
        libraries: [{ id: 1, name: 'Manga', scanned: true }],
        restricted: false,
      })
      const host = await settledAt('/search?q=hello&library=')
      expect(locationOf(host)).toBe('/?q=hello')
      expect(vi.mocked(library.search).mock.calls).toEqual([
        ['hello', undefined],
      ])
    })

    it('lands a link with no library on every library', async () => {
      const host = await settledAt('/search?q=hello')
      expect(locationOf(host)).toBe('/?q=hello')
    })

    it('lands a bare link on the Library view', async () => {
      const host = await settledAt('/search')
      expect(locationOf(host)).toBe('/')
    })
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
   * case: an account in a library with nothing readable.
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

  it('names the account screen’s tab Account — O173', async () => {
    document.title = 'stale'
    at('/account')
    await flush()
    expect(document.title).toBe('Account — taiyaku')
  })

  it('routes /about into the about screen inside the shell', async () => {
    const host = at('/about')
    await flush()
    expect(host.querySelector('[data-testid="reader"]')).toBeNull()
    expect(host.querySelector('.sidebar')).not.toBeNull()
    const screen = host.querySelector(
      '.library > main > section[aria-label="About"]',
    )
    expect(screen?.querySelector('h1')?.textContent).toBe('About')
    expect(screen?.querySelector('.about-facts')).not.toBeNull()
  })

  it('names the about screen’s tab About', async () => {
    document.title = 'stale'
    at('/about')
    await flush()
    expect(document.title).toBe('About — taiyaku')
  })

  /** The licence page, its texts landed. What it draws is `Licence.test.tsx`'s. */
  const licenceAt = async () => {
    const host = at('/about/licence')
    await licenceTextsLanded()
    return host
  }

  it('routes /about/licence into the licence screen inside the shell', async () => {
    const host = await licenceAt()
    expect(host.querySelector('[data-testid="reader"]')).toBeNull()
    expect(host.querySelector('.sidebar')).not.toBeNull()
    const screen = host.querySelector(
      '.library > main > section[aria-label="Licence"]',
    )
    expect(screen?.querySelector('h1')?.textContent).toBe('Licence')
  })

  it('names the licence screen’s tab Licence', async () => {
    document.title = 'stale'
    await licenceAt()
    expect(document.title).toBe('Licence — taiyaku')
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

  it('opens a book in the book reader, and leaving it returns to the library', async () => {
    const host = at('/book/harry-potter-1')
    expect(host.querySelector('[data-testid="book"]')?.textContent).toContain(
      'harry-potter-1',
    )
    expect(host.querySelector('[data-testid="reader"]')).toBeNull()
    await act(async () => {
      host
        .querySelector<HTMLButtonElement>('[data-testid="leave-book"]')
        ?.click()
      await Promise.resolve()
    })
    expect(host.querySelector('[data-testid="book"]')).toBeNull()
    await flush()
  })

  it('sends a book route with no work to the library', async () => {
    vi.mocked(useParams).mockReturnValueOnce({})
    const host = at('/book/harry-potter-1')
    expect(host.querySelector('[data-testid="book"]')).toBeNull()
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
   * not have to become re-entrant", so this test navigates from one
   * `/read/:work` URL to another to pin that the key does its job.
   * `App.test.tsx`'s "a changing work remounts" test proves React's key
   * semantics in general, with `rerender` supplying `key="one"`/`key="two"`
   * itself; it says nothing about whether `Reader` actually keys `<App>` by
   * the URL segment.
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
