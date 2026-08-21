import { act, useEffect } from 'react'
import { MemoryRouter, useNavigate, useParams } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { AppRoutes } from './Routes'
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
}))

const at = (path: string) =>
  mount(
    <MemoryRouter initialEntries={[path]}>
      <AppRoutes />
    </MemoryRouter>,
  )

describe('the four routes', () => {
  beforeEach(() => {
    mounts.length = 0
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

  it('does not mount the reader anywhere else', () => {
    const host = at('/')
    // Negative half: the reader is absent. On its own this would also pass
    // if the router rendered nothing at all, so the positive half below —
    // the library actually rendered — is what tells "the library is here
    // instead" from "nothing is here".
    expect(host.querySelector('[data-testid="reader"]')).toBeNull()
    expect(host.querySelector('.library')).not.toBeNull()
    expect(host.querySelector('.sidebar')).not.toBeNull()
  })

  it('answers an unknown path with the library rather than the reader', () => {
    const host = at('/nonsense')
    // Asserted structurally rather than by text: the `Library` this task
    // mounts is Task 8's placeholder and has no text content yet.
    expect(host.querySelector('main')).not.toBeNull()
    expect(host.querySelector('[data-testid="reader"]')).toBeNull()
  })

  it('builds a series page placeholder, named rather than blank', () => {
    const host = at('/series/1')
    expect(host.querySelector('h1')?.textContent).toBe('Series')
    expect(host.textContent).toContain('Not built yet.')
    expect(host.querySelector('[data-testid="reader"]')).toBeNull()
  })

  it('builds a search page placeholder, named rather than blank', () => {
    const host = at('/search')
    expect(host.querySelector('h1')?.textContent).toBe('Search')
    expect(host.textContent).toContain('Not built yet.')
  })

  it('sends a reader route with no :work back to the library rather than crashing', () => {
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
